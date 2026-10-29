<template>
  <div class="messages-page">
    <div v-if="loadError" class="notice notice-error" role="alert">{{ loadError }}</div>
    <div v-else-if="config && !config.enabled" class="notice">{{ t('messages.disabled') }}</div>

    <div v-else-if="config" class="messages-layout">
      <!-- Sidebar: Board, remembered/joined channels, join form -->
      <aside class="sidebar" :aria-label="t('messages.channels')">
        <ul class="channel-list">
          <li v-for="channel in channels" :key="channel.id">
            <button
              type="button"
              class="channel-item"
              :class="{ active: channel.id === activeId }"
              :aria-current="channel.id === activeId ? 'true' : undefined"
              @click="selectChannel(channel.id)"
            >
              <span class="channel-icon" aria-hidden="true">{{ channel.kind === 'public' ? '📌' : '🔒' }}</span>
              <span class="channel-name">{{ channel.label }}</span>
              <span v-if="channel.kind === 'public' && board.unread && activeId !== channel.id" class="badge">{{ board.unread }}</span>
              <span v-if="channel.kind === 'encrypted' && !channel.remembered" class="channel-session" :title="t('messages.notRemembered')">{{ t('messages.session') }}</span>
            </button>
          </li>
        </ul>

        <details class="join-details" :open="channels.length === 1">
        <summary>🔒 {{ t('messages.join.summary') }}</summary>
        <form class="join-form" @submit.prevent="joinChannel" autocomplete="off">
          <label>
            <span>{{ t('messages.join.name') }}</span>
            <input v-model="join.name" type="text" maxlength="80" required :placeholder="t('messages.join.namePlaceholder')" autocapitalize="off" />
          </label>
          <label>
            <span>{{ t('messages.join.passphrase') }}</span>
            <input v-model="join.passphrase" type="password" required minlength="8" :placeholder="t('messages.join.passphrasePlaceholder')" autocomplete="new-password" />
          </label>
          <label class="check">
            <input v-model="join.remember" type="checkbox" />
            <span>{{ t('messages.join.remember') }}</span>
          </label>
          <p v-if="join.passphrase && join.passphrase.length < 12" class="hint">{{ t('messages.join.shortWarning') }}</p>
          <button type="submit" class="btn btn-primary" :disabled="join.busy">
            {{ join.busy ? t('messages.join.deriving', { pct: Math.round(join.progress * 100) }) : t('messages.join.submit') }}
          </button>
          <div v-if="join.notFound" class="create-offer">
            <p>{{ t('messages.join.notFound', { name: join.notFound.name }) }}</p>
            <button type="button" class="btn btn-primary" :disabled="join.busy || !config.can_post" @click="createChannel">{{ t('messages.join.create') }}</button>
            <p v-if="!config.can_post" class="hint">{{ t('messages.join.cannotCreate') }}</p>
          </div>
          <p v-if="join.error" class="form-error" role="alert">{{ join.error }}</p>
        </form>
        </details>

        <details class="join-details">
          <summary>🔑 {{ t('messages.keys.summary') }}</summary>
          <div class="join-form">
            <p class="hint">{{ t('messages.keys.intro') }}</p>
            <label>
              <span>{{ t('messages.keys.filePassphrase') }}</span>
              <input v-model="keyTransfer.passphrase" type="password" minlength="8" autocomplete="new-password" :placeholder="t('messages.keys.filePassphrasePlaceholder')" />
            </label>
            <button type="button" class="btn btn-primary" :disabled="keyTransfer.busy || !rememberedCount" @click="exportKeys">
              {{ keyTransfer.busy === 'export' ? t('messages.keys.encrypting', { pct: Math.round(keyTransfer.progress * 100) }) : t('messages.keys.export', { count: rememberedCount }, rememberedCount) }}
            </button>
            <label>
              <span>{{ t('messages.keys.importFile') }}</span>
              <input ref="keyFileInput" type="file" accept=".json,application/json" @change="onKeyFile" />
            </label>
            <button type="button" class="btn btn-ghost" :disabled="keyTransfer.busy || !keyTransfer.file" @click="importKeys">
              {{ keyTransfer.busy === 'import' ? t('messages.keys.decrypting', { pct: Math.round(keyTransfer.progress * 100) }) : t('messages.keys.import') }}
            </button>
            <p v-if="keyTransfer.status" class="hint" role="status">{{ keyTransfer.status }}</p>
            <p v-if="keyTransfer.error" class="form-error" role="alert">{{ keyTransfer.error }}</p>
          </div>
        </details>

        <div class="identity">
          <label>
            <span>{{ t('messages.nickname') }}</span>
            <input v-model="nickname" type="text" maxlength="40" :placeholder="t('messages.nicknamePlaceholder')" @change="saveNickname" />
          </label>
          <p class="hint">
            {{ t('messages.deviceTag') }}
            <span class="dot" :style="{ background: deviceColor(deviceTag) }" aria-hidden="true"></span>
            <code>{{ deviceTag }}</code>
          </p>
        </div>
      </aside>

      <!-- Main pane -->
      <section class="main-pane" :aria-label="activeChannel ? activeChannel.label : t('nav.messages')">
        <header class="pane-header" v-if="activeChannel">
          <div class="pane-title">
            <h3>{{ activeChannel.label }}</h3>
            <span class="conn" :class="`conn-${connection}`" role="status">{{ connectionLabel }}</span>
          </div>
          <div class="pane-actions">
            <button type="button" class="btn btn-ghost" :aria-expanded="String(showInfo)" @click="showInfo = !showInfo">ⓘ {{ t('messages.security') }}</button>
            <template v-if="activeChannel.kind === 'encrypted'">
              <button type="button" class="btn btn-ghost" :aria-expanded="String(showShare)" @click="showShare = !showShare">{{ t('messages.share') }}</button>
              <button v-if="activeChannel.remembered" type="button" class="btn btn-ghost" @click="forgetChannel(activeChannel)">{{ t('messages.forget') }}</button>
              <button v-else type="button" class="btn btn-ghost" @click="rememberChannel(activeChannel)">{{ t('messages.remember') }}</button>
              <button v-if="config.can_moderate" type="button" class="btn btn-danger" @click="deleteChannel(activeChannel)">{{ t('messages.deleteChannel') }}</button>
            </template>
          </div>
        </header>

        <div v-if="showInfo" class="info-popover" role="note">
          <h4>{{ t('messages.info.title') }}</h4>
          <template v-if="activeChannel?.kind === 'public'">
            <i18n-t keypath="messages.info.board" tag="p" scope="global">
              <template #public><strong>{{ t('messages.info.boardPublic') }}</strong></template>
            </i18n-t>
          </template>
          <ul>
            <li>{{ t('messages.info.encrypted') }}</li>
            <li><strong>{{ t('messages.info.protectsLabel') }}</strong> {{ t('messages.info.protects') }}</li>
            <li><strong>{{ t('messages.info.notProtectsLabel') }}</strong> {{ t('messages.info.notProtects') }}</li>
            <li>{{ t('messages.info.anyone') }}</li>
            <li>{{ t('messages.info.weak') }}</li>
          </ul>
        </div>

        <div v-if="showShare && activeChannel?.kind === 'encrypted'" class="share-panel">
          <i18n-t keypath="messages.shareLink.intro" tag="p" scope="global">
            <template #hash><code>#</code></template>
          </i18n-t>
          <label class="check warn">
            <input v-model="shareIncludePassphrase" type="checkbox" :disabled="!sessionPassphrase(activeChannel.id)" />
            <span>{{ t('messages.shareLink.includePassphrase') }}</span>
          </label>
          <p v-if="!sessionPassphrase(activeChannel.id)" class="hint">{{ t('messages.shareLink.noPassphrase') }}</p>
          <div class="share-row">
            <input class="share-link" :value="shareLink" readonly @focus="$event.target.select()" />
            <button type="button" class="btn btn-ghost" @click="copyShareLink">{{ copied ? t('common.copied') : t('common.copy') }}</button>
          </div>
          <img v-if="shareQr" class="qr" :src="shareQr" :alt="t('messages.shareLink.qrAlt')" width="180" height="180" />
        </div>

        <div v-if="activeChannel?.kind === 'public' && pinned.length" class="pinned">
          <h4>📌 {{ t('messages.pinned') }}</h4>
          <article v-for="m in pinned" :key="`p${m.id}`" class="pinned-item">
            <span v-if="m.category" class="chip" :class="`chip-${m.category}`">{{ categoryLabel(m.category) }}</span>
            <strong>{{ m.nick }}</strong>:
            <template v-for="(seg, i) in linkify(m.text)" :key="i"><a v-if="seg.type === 'link'" :href="seg.href" target="_blank" rel="noopener noreferrer nofollow">{{ seg.text }}</a><span v-else>{{ seg.text }}</span></template>
            <button v-if="config.can_moderate" type="button" class="link-btn" @click="setPinned(m, false)">{{ t('messages.unpin') }}</button>
          </article>
        </div>

        <div ref="listEl" class="message-list" aria-live="polite" @scroll="onScroll">
          <button v-if="hasOlder" type="button" class="btn btn-ghost load-older" :disabled="loadingOlder" @click="loadOlder">{{ t('messages.loadOlder') }}</button>
          <p v-if="channelError" class="notice notice-error">
            {{ channelError }}
            <button v-if="channelMissing && config.can_post" type="button" class="btn btn-ghost" @click="recreateChannel">{{ t('messages.recreate') }}</button>
          </p>
          <p v-else-if="!loadingMessages && !messages.length" class="empty">{{ t('messages.empty') }}</p>
          <section v-for="group in groups" :key="group.key" class="day-group">
            <h4 class="day-label"><span>{{ group.label }}</span></h4>
            <article
              v-for="m in group.messages"
              :key="m.id"
              class="message"
              :class="{ own: m.own, undecryptable: m.undecryptable, alert: m.category === 'alert' }"
            >
              <div class="meta">
                <span class="dot" :style="{ background: deviceColor(m.device_tag) }" :title="t('messages.device', { tag: m.device_tag })" aria-hidden="true"></span>
                <span class="nick">{{ m.nick }}</span>
                <span v-if="m.category" class="chip" :class="`chip-${m.category}`">{{ categoryLabel(m.category) }}</span>
                <time :datetime="new Date(m.created_at).toISOString()">{{ timeLabel(m.created_at) }}</time>
                <span v-if="m.pinned" :title="t('messages.pinned')" :aria-label="t('messages.pinned')">📌</span>
              </div>
              <p v-if="m.undecryptable" class="text muted">{{ t('messages.undecryptable') }}</p>
              <p v-else class="text"><template v-for="(seg, i) in linkify(m.text)" :key="i"><a v-if="seg.type === 'link'" :href="seg.href" target="_blank" rel="noopener noreferrer nofollow">{{ seg.text }}</a><span v-else>{{ seg.text }}</span></template></p>
              <router-link v-if="m.lat != null && m.lng != null" class="location" :to="mapsLink({ lat: m.lat, lng: m.lng, label: m.text })">📍 {{ formatCoords(m.lat, m.lng) }}</router-link>
              <div v-if="config.can_moderate" class="mod-actions">
                <button v-if="activeChannel?.kind === 'public'" type="button" class="link-btn" @click="setPinned(m, !m.pinned)">{{ m.pinned ? t('messages.unpin') : t('messages.pin') }}</button>
                <button type="button" class="link-btn danger" @click="deleteMessage(m)">{{ t('common.delete') }}</button>
              </div>
            </article>
          </section>
        </div>

        <form v-if="config.can_post && activeChannel && !channelError" class="composer" @submit.prevent="send">
          <input
            v-if="!hasNickname"
            v-model="nickname"
            type="text"
            maxlength="40"
            :placeholder="t('messages.composer.nicknamePlaceholder')"
            :aria-label="t('messages.nickname')"
            @change="saveNickname"
          />
          <div class="composer-options">
            <select v-model="draft.category" :aria-label="t('messages.composer.category')">
              <option value="">{{ t('messages.composer.chat') }}</option>
              <option v-for="c in CATEGORIES" :key="c" :value="c">{{ categoryLabel(c) }}</option>
            </select>
            <button type="button" class="btn btn-ghost" @click="toggleLocation">{{ draft.location ? `📍 ${formatCoords(draft.location.lat, draft.location.lng)} ✕` : `📍 ${t('messages.composer.addLocation')}` }}</button>
            <select v-if="activeChannel.kind === 'public'" v-model.number="draft.ttlDays" :aria-label="t('messages.composer.keepFor')">
              <option :value="1">{{ t('messages.composer.keepDays', { count: 1 }, 1) }}</option>
              <option :value="7">{{ t('messages.composer.keepDays', { count: 7 }, 7) }}</option>
              <option :value="30">{{ t('messages.composer.keepDays', { count: 30 }, 30) }}</option>
              <option :value="90">{{ t('messages.composer.keepDays', { count: 90 }, 90) }}</option>
              <option :value="365">{{ t('messages.composer.keepYear') }}</option>
            </select>
          </div>
          <p v-if="locationError" class="form-error">{{ locationError }}</p>
          <div class="composer-row">
            <textarea
              ref="composerEl"
              v-model="draft.text"
              rows="2"
              :maxlength="config.limits.max_text_chars"
              :placeholder="nickname ? t('messages.composer.placeholder', { nick: nickname }) : t('messages.composer.placeholderNoNick')"
              :aria-label="t('messages.composer.message')"
              @keydown.enter.exact.prevent="send"
            ></textarea>
            <button type="submit" class="btn btn-primary" :disabled="sending || !draft.text.trim() || !nickname.trim()">{{ t('messages.composer.send') }}</button>
          </div>
          <p class="composer-foot">
            <span>{{ charCount(draft.text) }}/{{ config.limits.max_text_chars }}</span>
            <span v-if="activeChannel.kind === 'public'">{{ t('messages.composer.public') }}</span>
            <span v-else>🔒 {{ t('messages.composer.e2e') }}</span>
          </p>
          <p v-if="sendError" class="form-error" role="alert">{{ sendError }}</p>
        </form>
        <p v-else-if="config && !config.can_post" class="notice">
          {{ readOnlyReason }}
        </p>
      </section>
    </div>

    <div v-else class="notice">{{ t('messages.loading') }}</div>
  </div>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { renderSVG } from 'uqr'
import { useI18n } from 'vue-i18n'
import { errorText } from '../i18n/index.js'
import { apiService } from '../services/api'
import { useAuthState } from '../services/auth'
import { useLocationState } from '../services/location'
import {
  buildPayload,
  channelVerifier,
  deriveChannelKeys,
  exportChannelKeys,
  importChannelKeys,
  encryptMessage,
  keyFromHex,
  keyToHex
} from '../modules/messaging/crypto.js'
import { CATEGORIES, categoryLabel, charCount, deviceColor, formatCoords, groupByDay, linkify, mapsLink, timeLabel } from '../modules/messaging/format.js'
import { PUBLIC_CHANNEL_ID, dropExpired, latestId, mergeMessages, removeMessage, toViewMessage } from '../modules/messaging/model.js'
import { messagingStorage } from '../modules/messaging/storage.js'
import { createChannelStream } from '../modules/messaging/stream.js'
import { markBoardSeen, setBoardViewing, startBoardFeed, subscribeBoard, useBoardFeed } from '../modules/messaging/boardFeed.js'
import { datedFilename, readFileText, saveText } from '../services/fileTransfer'

const storage = messagingStorage()
const authState = useAuthState()
const locationState = useLocationState()
const board = useBoardFeed()

const config = ref(null)
const { t } = useI18n()
const loadError = ref('')
const nickname = ref(storage.nickname())
const hasNickname = ref(Boolean(nickname.value))
const deviceTag = storage.deviceTag()

// Channels: Board first, then encrypted channels (remembered + this session).
// Keys of session-only channels live only in this in-memory map.
const sessionChannels = reactive(new Map())
const sessionPassphrases = new Map()
const rememberedVersion = ref(0)
const channels = computed(() => {
  void rememberedVersion.value
  const list = [{ id: PUBLIC_CHANNEL_ID, label: t('messages.board'), kind: 'public' }]
  const remembered = storage.channels()
  for (const c of remembered) {
    list.push({ id: c.channelId, label: c.name, kind: 'encrypted', remembered: true, authKeyHex: c.authKey, encKeyHex: c.encKey })
  }
  for (const c of sessionChannels.values()) {
    if (!remembered.some((r) => r.channelId === c.id)) list.push(c)
  }
  return list
})

const activeId = ref(PUBLIC_CHANNEL_ID)
const activeChannel = computed(() => channels.value.find((c) => c.id === activeId.value) || null)

const messages = ref([])
const pinned = ref([])
const hasOlder = ref(false)
const loadingMessages = ref(false)
const loadingOlder = ref(false)
const channelError = ref('')
const channelMissing = ref(false)
const connection = ref('connecting')
const listEl = ref(null)
const composerEl = ref(null)
let stickToBottom = true
let unsubscribeBoard = null
let channelStream = null
let loadToken = 0
let expiryTimer = null

const showInfo = ref(false)
const showShare = ref(false)
const shareIncludePassphrase = ref(false)
const copied = ref(false)

const join = reactive({ name: '', passphrase: '', remember: true, busy: false, progress: 0, error: '', notFound: null })
const draft = reactive({ text: '', category: '', location: null, ttlDays: 30 })
const sending = ref(false)
const sendError = ref('')
const locationError = ref('')

const groups = computed(() => groupByDay(messages.value))
const connectionLabel = computed(() => ({
  connecting: t('messages.connection.connecting'),
  live: t('messages.connection.live'),
  reconnecting: t('messages.connection.reconnecting'),
  stopped: t('messages.connection.stopped'),
  error: t('messages.connection.error')
}[connection.value] || connection.value))

const readOnlyReason = computed(() => {
  if (authState.readonly) return t('messages.readOnlyServer')
  return t('messages.loginToPost')
})

const keyCtx = (channel) => ({
  ownIds: storage.ownIds(channel?.id),
  encKey: channel?.encKeyHex ? keyFromHex(channel.encKeyHex) : undefined
})
const keyHeader = (channel) => (channel?.kind === 'encrypted' ? channel.authKeyHex : null)

// ---------------------------------------------------------------------------
// Loading and live updates
// ---------------------------------------------------------------------------

const scrollToBottom = async (force = false) => {
  await nextTick()
  const el = listEl.value
  if (el && (force || stickToBottom)) el.scrollTop = el.scrollHeight
}

const onScroll = () => {
  const el = listEl.value
  if (!el) return
  stickToBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 80
}

const markSeen = () => {
  const channel = activeChannel.value
  const last = latestId(messages.value)
  if (!channel || !last) return
  if (channel.kind === 'public') markBoardSeen(last)
  else storage.setLastSeen(channel.id, last)
}

const ingest = (rows, channel) => {
  const view = rows.map((row) => toViewMessage(row, keyCtx(channel)))
  messages.value = dropExpired(mergeMessages(messages.value, view))
  markSeen()
  scrollToBottom()
}

const catchUp = async (channel, token) => {
  let since = latestId(messages.value)
  for (let page = 0; page < 10; page += 1) {
    const data = await apiService.listChannelMessages(channel.id, since ? { since, limit: 200 } : { limit: 100 }, keyHeader(channel))
    if (token !== loadToken) return
    ingest(data.messages || [], channel)
    if (channel.kind === 'public' && !since) pinned.value = (data.pinned || []).map((r) => toViewMessage(r, keyCtx(channel)))
    if (!since) hasOlder.value = Boolean(data.has_more)
    if (!since || !data.has_more) return
    since = latestId(messages.value)
  }
}

const handleEvent = (channel, name, data) => {
  if (activeChannel.value?.id !== channel.id) return
  if (name === 'message') ingest([data], channel)
  else if (name === 'deleted') {
    messages.value = removeMessage(messages.value, data?.id)
    pinned.value = removeMessage(pinned.value, data?.id)
  } else if (name === 'pinned') {
    const view = toViewMessage(data, keyCtx(channel))
    if (messages.value.some((m) => m.id === view.id)) messages.value = mergeMessages(messages.value, [view])
    pinned.value = view.pinned ? mergeMessages(pinned.value, [view]) : removeMessage(pinned.value, view.id)
  } else if (name === 'resync') {
    catchUp(channel, loadToken).catch(() => {})
  } else if (name === 'channel_deleted') {
    channelError.value = t('messages.errors.channelDeleted')
    channelMissing.value = true
  }
}

const describeError = (err) => {
  const status = err?.response?.status
  if (status === 401) return t('messages.errors.unauthorized')
  if (status === 403) return authState.readonly ? t('messages.errors.readonly') : t('messages.errors.forbidden')
  if (status === 404) return t('messages.errors.notFound')
  if (status === 413 || status === 422) return err?.response?.data?.error || t('messages.errors.rejected')
  if (status === 429) return t('messages.errors.tooFast')
  return err?.response?.data?.error || apiService.handleError(err)
}

const stopChannel = () => {
  loadToken += 1
  unsubscribeBoard?.()
  unsubscribeBoard = null
  channelStream?.stop()
  channelStream = null
  setBoardViewing(false)
}

const openChannel = async (channel) => {
  stopChannel()
  const token = loadToken
  messages.value = []
  pinned.value = []
  hasOlder.value = false
  channelError.value = ''
  channelMissing.value = false
  showShare.value = false
  shareIncludePassphrase.value = false
  stickToBottom = true
  loadingMessages.value = true
  try {
    await catchUp(channel, token)
  } catch (err) {
    if (token === loadToken) {
      channelError.value = describeError(err)
      channelMissing.value = channel.kind === 'encrypted' && err?.response?.status === 404
    }
  } finally {
    if (token === loadToken) loadingMessages.value = false
  }
  if (token !== loadToken || channelError.value) return
  scrollToBottom(true)

  if (channel.kind === 'public') {
    setBoardViewing(true)
    unsubscribeBoard = subscribeBoard({
      onEvent: (name, data) => handleEvent(channel, name, data),
      onStatus: (status) => { if (token === loadToken) connection.value = status },
      onCatchUp: () => (token === loadToken ? catchUp(channel, token) : undefined)
    })
  } else {
    channelStream = createChannelStream({
      url: apiService.channelStreamUrl(channel.id),
      headers: { 'X-Fyr-Channel-Key': channel.authKeyHex },
      onEvent: (name, data) => handleEvent(channel, name, data),
      onStatus: (status, detail) => {
        if (token !== loadToken) return
        connection.value = status
        if (status === 'error' && detail?.status && !channelError.value) channelError.value = describeError({ response: { status: detail.status } })
      },
      onCatchUp: () => catchUp(channel, token)
    })
    channelStream.start()
  }
}

const selectChannel = (id) => {
  if (id === activeId.value) return
  activeId.value = id
}

watch(activeChannel, (channel, previous) => {
  if (channel && channel.id !== previous?.id) openChannel(channel)
})

const loadOlder = async () => {
  const channel = activeChannel.value
  if (!channel || !messages.value.length) return
  loadingOlder.value = true
  const el = listEl.value
  const before = el ? el.scrollHeight - el.scrollTop : 0
  try {
    const data = await apiService.listChannelMessages(channel.id, { before: messages.value[0].id, limit: 100 }, keyHeader(channel))
    messages.value = mergeMessages(messages.value, (data.messages || []).map((r) => toViewMessage(r, keyCtx(channel))), 5000)
    hasOlder.value = Boolean(data.has_more)
    await nextTick()
    if (el) el.scrollTop = el.scrollHeight - before
  } catch (err) {
    channelError.value = describeError(err)
  } finally {
    loadingOlder.value = false
  }
}

// ---------------------------------------------------------------------------
// Posting
// ---------------------------------------------------------------------------

const saveNickname = () => {
  storage.setNickname(nickname.value)
  nickname.value = storage.nickname()
  hasNickname.value = Boolean(nickname.value)
}

const toggleLocation = () => {
  locationError.value = ''
  if (draft.location) {
    draft.location = null
    return
  }
  const fromSettings = locationState.location
  const useSaved = () => {
    if (fromSettings && Number.isFinite(fromSettings.latitude)) {
      draft.location = { lat: Number(fromSettings.latitude.toFixed(5)), lng: Number(fromSettings.longitude.toFixed(5)) }
    } else {
      locationError.value = t('messages.errors.locationUnavailable')
    }
  }
  if (globalThis.isSecureContext && navigator.geolocation) {
    navigator.geolocation.getCurrentPosition(
      (pos) => { draft.location = { lat: Number(pos.coords.latitude.toFixed(5)), lng: Number(pos.coords.longitude.toFixed(5)) } },
      useSaved,
      { enableHighAccuracy: true, timeout: 10000, maximumAge: 60000 }
    )
  } else {
    useSaved()
  }
}

const send = async () => {
  const channel = activeChannel.value
  const text = draft.text.trim()
  if (!channel || !text || sending.value) return
  saveNickname()
  if (!nickname.value) {
    sendError.value = t('messages.errors.nicknameFirst')
    return
  }
  if (charCount(text) > config.value.limits.max_text_chars) {
    sendError.value = t('messages.errors.tooLong')
    return
  }
  sending.value = true
  sendError.value = ''
  try {
    let row
    if (channel.kind === 'public') {
      const body = { nick: nickname.value, device_tag: deviceTag, text, ttl_days: draft.ttlDays }
      if (draft.category) body.category = draft.category
      if (draft.location) Object.assign(body, draft.location)
      row = await apiService.postChannelMessage(channel.id, body)
    } else {
      const payload = buildPayload({
        nick: nickname.value,
        deviceTag,
        text,
        category: draft.category || undefined,
        lat: draft.location?.lat,
        lng: draft.location?.lng
      })
      const sealed = encryptMessage(keyFromHex(channel.encKeyHex), channel.id, payload)
      row = await apiService.postChannelMessage(channel.id, sealed, channel.authKeyHex)
    }
    storage.addOwnId(channel.id, row.id)
    stickToBottom = true
    ingest([row], channel)
    draft.text = ''
    draft.location = null
    draft.category = ''
    composerEl.value?.focus()
  } catch (err) {
    sendError.value = err?.response ? describeError(err) : errorText(err, 'messages.errors.sendFailed')
  } finally {
    sending.value = false
  }
}

// ---------------------------------------------------------------------------
// Joining / creating / remembering channels
// ---------------------------------------------------------------------------

const addSessionChannel = (keys, remember, passphrase) => {
  const entry = {
    id: keys.channelId,
    label: keys.name,
    kind: 'encrypted',
    remembered: false,
    authKeyHex: keyToHex(keys.authKey),
    encKeyHex: keyToHex(keys.encKey)
  }
  sessionChannels.set(entry.id, entry)
  sessionPassphrases.set(entry.id, passphrase)
  if (remember) rememberChannel(entry)
  activeId.value = entry.id
}

const deriveFromForm = async () => {
  join.progress = 0
  return deriveChannelKeys({
    name: join.name,
    passphrase: join.passphrase,
    serverSalt: config.value.server_salt,
    onProgress: (p) => { join.progress = p }
  })
}

let pendingKeys = null

const joinChannel = async () => {
  join.error = ''
  join.notFound = null
  if (join.passphrase.length < 8) {
    join.error = t('messages.errors.shortPassphrase', { min: 8 })
    return
  }
  join.busy = true
  try {
    const keys = await deriveFromForm()
    try {
      await apiService.listChannelMessages(keys.channelId, { limit: 1 }, keyToHex(keys.authKey))
      addSessionChannel(keys, join.remember, join.passphrase)
      join.passphrase = ''
      join.name = ''
    } catch (err) {
      if (err?.response?.status === 404) {
        pendingKeys = keys
        join.notFound = { name: keys.name }
      } else {
        join.error = describeError(err)
      }
    }
  } catch (err) {
    join.error = errorText(err, 'messages.errors.deriveFailed')
  } finally {
    join.busy = false
  }
}

const createChannel = async () => {
  if (!pendingKeys) return
  join.busy = true
  join.error = ''
  try {
    await apiService.createChannel(pendingKeys.channelId, channelVerifier(pendingKeys.authKey))
  } catch (err) {
    if (err?.response?.status !== 409) {
      join.error = describeError(err)
      join.busy = false
      return
    }
  }
  addSessionChannel(pendingKeys, join.remember, join.passphrase)
  pendingKeys = null
  join.notFound = null
  join.passphrase = ''
  join.name = ''
  join.busy = false
}

// Channel ids are deterministic, so an expired (empty for a week) or
// deleted channel can be registered again with the keys we already hold.
const recreateChannel = async () => {
  const channel = activeChannel.value
  if (!channel || channel.kind !== 'encrypted') return
  try {
    await apiService.createChannel(channel.id, channelVerifier(keyFromHex(channel.authKeyHex)))
  } catch (err) {
    if (err?.response?.status !== 409) {
      channelError.value = describeError(err)
      return
    }
  }
  openChannel(channel)
}

const rememberChannel = (channel) => {
  try {
    storage.rememberChannel({ name: channel.label, channelId: channel.id, authKey: channel.authKeyHex, encKey: channel.encKeyHex })
  } catch {
    // Storage blocked: channel stays session-only.
  }
  rememberedVersion.value += 1
}

const forgetChannel = (channel) => {
  storage.forgetChannel(channel.id)
  sessionChannels.delete(channel.id)
  sessionPassphrases.delete(channel.id)
  rememberedVersion.value += 1
  activeId.value = PUBLIC_CHANNEL_ID
}

// Encrypted export/import of remembered channel keys (client-side only).
const keyTransfer = reactive({ passphrase: '', file: null, busy: '', progress: 0, status: '', error: '' })
const keyFileInput = ref(null)
const rememberedCount = computed(() => {
  void rememberedVersion.value
  return storage.channels().length
})

const exportKeys = async () => {
  keyTransfer.error = ''
  keyTransfer.status = ''
  keyTransfer.busy = 'export'
  keyTransfer.progress = 0
  try {
    const text = await exportChannelKeys(storage.channels(), keyTransfer.passphrase, {
      onProgress: (p) => { keyTransfer.progress = p }
    })
    saveText(text, datedFilename('channel-keys'))
    keyTransfer.status = t('messages.keys.saved')
    keyTransfer.passphrase = ''
  } catch (err) {
    keyTransfer.error = errorText(err, 'messages.keys.exportFailed')
  } finally {
    keyTransfer.busy = ''
  }
}

const onKeyFile = (event) => {
  keyTransfer.file = event.target.files?.[0] || null
  keyTransfer.status = ''
  keyTransfer.error = ''
}

const importKeys = async () => {
  keyTransfer.error = ''
  keyTransfer.status = ''
  keyTransfer.busy = 'import'
  keyTransfer.progress = 0
  try {
    const text = await readFileText(keyTransfer.file, 256 * 1024)
    const { channels: imported } = await importChannelKeys(text, keyTransfer.passphrase, {
      onProgress: (p) => { keyTransfer.progress = p }
    })
    let count = 0
    for (const entry of imported) {
      try {
        storage.rememberChannel(entry)
        count += 1
      } catch {
        // invalid entry: skipped
      }
    }
    rememberedVersion.value += 1
    keyTransfer.status = t('messages.keys.imported', { count }, count)
    keyTransfer.file = null
    keyTransfer.passphrase = ''
    if (keyFileInput.value) keyFileInput.value.value = ''
  } catch (err) {
    keyTransfer.error = errorText(err, 'messages.keys.importFailed')
  } finally {
    keyTransfer.busy = ''
  }
}

const sessionPassphrase = (id) => sessionPassphrases.get(id) || ''

const shareLink = computed(() => {
  const channel = activeChannel.value
  if (!channel || channel.kind !== 'encrypted') return ''
  const params = new URLSearchParams({ join: channel.label })
  const pass = sessionPassphrase(channel.id)
  if (shareIncludePassphrase.value && pass) params.set('pass', pass)
  return `${window.location.origin}/messages#${params.toString()}`
})

const shareQr = computed(() => {
  if (!shareLink.value) return ''
  try {
    const svg = renderSVG(shareLink.value, { ecc: 'M', border: 2 })
    return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`
  } catch {
    return ''
  }
})

const copyShareLink = async () => {
  try {
    await navigator.clipboard.writeText(shareLink.value)
    copied.value = true
    setTimeout(() => { copied.value = false }, 1500)
  } catch {
    // Clipboard API needs a secure context; the field is selectable instead.
  }
}

const applyFragment = () => {
  const hash = window.location.hash.replace(/^#/, '')
  if (!hash) return
  const params = new URLSearchParams(hash)
  const name = params.get('join')
  if (name) {
    join.name = name.slice(0, 80)
    join.passphrase = params.get('pass') || ''
  }
  // Remove the fragment (it may contain a passphrase) from the address bar/history.
  history.replaceState(history.state, '', window.location.pathname + window.location.search)
}

// ---------------------------------------------------------------------------
// Moderation (admin only; the server enforces this)
// ---------------------------------------------------------------------------

const deleteMessage = async (message) => {
  if (!window.confirm(t('messages.confirmDeleteMessage'))) return
  try {
    await apiService.deleteChannelMessage(activeChannel.value.id, message.id)
    messages.value = removeMessage(messages.value, message.id)
    pinned.value = removeMessage(pinned.value, message.id)
  } catch (err) {
    sendError.value = describeError(err)
  }
}

const setPinned = async (message, value) => {
  try {
    const row = await apiService.pinChannelMessage(message.id, value)
    handleEvent(activeChannel.value, 'pinned', row)
  } catch (err) {
    sendError.value = describeError(err)
  }
}

const deleteChannel = async (channel) => {
  if (!window.confirm(t('messages.confirmDeleteChannel', { name: channel.label }))) return
  try {
    await apiService.deleteChannel(channel.id)
    forgetChannel(channel)
  } catch (err) {
    channelError.value = describeError(err)
  }
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

onMounted(async () => {
  try {
    config.value = await apiService.getMessagesConfig()
  } catch (err) {
    loadError.value = apiService.handleError(err)
    return
  }
  if (!config.value.enabled) return
  applyFragment()
  await startBoardFeed()
  openChannel(activeChannel.value)
  expiryTimer = window.setInterval(() => {
    messages.value = dropExpired(messages.value)
  }, 60000)
})

onBeforeUnmount(() => {
  stopChannel()
  if (expiryTimer) window.clearInterval(expiryTimer)
})
</script>

<style scoped>
.messages-page {
  --panel: #2a2a2a;
  --panel-2: #1f1f1f;
  --line: #3a3a3a;
  --accent: #667eea;
  --muted: #9a9a9a;
  color: #e0e0e0;
}

.notice {
  padding: 1rem;
  background: var(--panel);
  border-radius: 8px;
  color: #b0b0b0;
}

.notice-error {
  background: #3d2a2a;
  color: #ff8b8b;
  border-left: 4px solid #ff6b6b;
}

.messages-layout {
  display: grid;
  grid-template-columns: minmax(220px, 280px) 1fr;
  gap: 1rem;
  min-height: 70vh;
}

.sidebar {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  min-width: 0;
}

.channel-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.channel-item {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.6rem 0.75rem;
  background: var(--panel);
  border: 1px solid transparent;
  border-radius: 6px;
  color: inherit;
  cursor: pointer;
  text-align: left;
  font: inherit;
}

.channel-item.active {
  border-color: var(--accent);
  background: #2f3150;
}

.channel-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.channel-session {
  font-size: 0.7rem;
  color: var(--muted);
}

.badge {
  background: #e05d5d;
  color: #fff;
  border-radius: 999px;
  padding: 0 0.45rem;
  font-size: 0.75rem;
  font-weight: 700;
}

.join-form,
.identity {
  background: var(--panel);
  border-radius: 8px;
  padding: 0.85rem;
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
}

.join-details {
  background: var(--panel);
  border-radius: 8px;
}

.join-details summary {
  cursor: pointer;
  padding: 0.7rem 0.85rem;
  font-size: 0.88rem;
  color: #c9c9c9;
}

.join-details .join-form {
  padding-top: 0;
}

label {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  font-size: 0.82rem;
  color: #b0b0b0;
}

label.check {
  flex-direction: row;
  align-items: flex-start;
  gap: 0.45rem;
}

label.warn span {
  color: #f5c26b;
}

input[type='text'],
input[type='password'],
.share-link,
select,
textarea {
  background: var(--panel-2);
  border: 1px solid var(--line);
  color: #e0e0e0;
  border-radius: 6px;
  padding: 0.5rem 0.6rem;
  font: inherit;
  font-size: 0.95rem;
  min-width: 0;
}

textarea {
  resize: vertical;
  flex: 1;
  min-height: 2.6rem;
}

.btn {
  border: none;
  border-radius: 6px;
  padding: 0.5rem 0.85rem;
  cursor: pointer;
  font: inherit;
  font-size: 0.88rem;
}

.btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.btn-primary {
  background: var(--accent);
  color: #fff;
}

.btn-ghost {
  background: rgba(255, 255, 255, 0.08);
  color: #e0e0e0;
}

.btn-danger {
  background: #7a2e2e;
  color: #fff;
}

.link-btn {
  background: none;
  border: none;
  color: #8db2ff;
  cursor: pointer;
  font-size: 0.78rem;
  padding: 0 0.25rem;
}

.link-btn.danger {
  color: #ff8b8b;
}

.hint {
  margin: 0;
  font-size: 0.78rem;
  color: var(--muted);
  display: flex;
  align-items: center;
  gap: 0.35rem;
  flex-wrap: wrap;
}

.form-error {
  margin: 0;
  color: #ff8b8b;
  font-size: 0.82rem;
}

.create-offer {
  border-top: 1px solid var(--line);
  padding-top: 0.5rem;
  font-size: 0.85rem;
}

.create-offer p {
  margin: 0 0 0.4rem;
}

.main-pane {
  display: flex;
  flex-direction: column;
  background: var(--panel);
  border-radius: 8px;
  min-width: 0;
  min-height: 70vh;
  max-height: calc(100vh - 12rem);
}

.pane-header {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 1rem;
  border-bottom: 1px solid var(--line);
}

.pane-title {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  min-width: 0;
}

.pane-title h3 {
  margin: 0;
  font-size: 1.05rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pane-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
}

.conn {
  font-size: 0.72rem;
  padding: 0.1rem 0.5rem;
  border-radius: 999px;
  background: #3a3a3a;
  color: #cfcfcf;
}

.conn-live {
  background: #2d5a2d;
  color: #a6f0a6;
}

.conn-reconnecting,
.conn-connecting {
  background: #5a4a2d;
  color: #f5d28b;
}

.conn-error {
  background: #5a2d2d;
  color: #ffb0b0;
}

.info-popover,
.share-panel,
.pinned {
  margin: 0.75rem 1rem 0;
  padding: 0.75rem 0.9rem;
  border-radius: 6px;
  background: var(--panel-2);
  border: 1px solid var(--line);
  font-size: 0.85rem;
}

.info-popover h4,
.pinned h4 {
  margin: 0 0 0.4rem;
  font-size: 0.85rem;
}

.info-popover ul {
  margin: 0;
  padding-left: 1.1rem;
  display: grid;
  gap: 0.3rem;
}

.share-row {
  display: flex;
  gap: 0.4rem;
  margin: 0.5rem 0;
}

.share-link {
  flex: 1;
}

.qr {
  background: #fff;
  border-radius: 6px;
  padding: 4px;
}

.pinned-item {
  padding: 0.25rem 0;
  word-break: break-word;
}

.message-list {
  flex: 1;
  overflow-y: auto;
  padding: 0.75rem 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.load-older {
  align-self: center;
}

.empty {
  color: var(--muted);
  text-align: center;
  margin: 2rem 0;
}

.day-group {
  display: flex;
  flex-direction: column;
}

.day-label {
  text-align: center;
  margin: 0.75rem 0 0.4rem;
  font-size: 0.75rem;
  color: var(--muted);
  font-weight: 500;
}

.day-label span {
  background: var(--panel-2);
  padding: 0.15rem 0.6rem;
  border-radius: 999px;
}

.message {
  max-width: min(80%, 40rem);
  align-self: flex-start;
  background: #333;
  border-radius: 10px 10px 10px 2px;
  padding: 0.45rem 0.7rem;
  margin-bottom: 0.35rem;
  word-break: break-word;
}

.message.own {
  align-self: flex-end;
  background: #34386a;
  border-radius: 10px 10px 2px 10px;
}

.message.alert {
  border-left: 3px solid #e05d5d;
}

.message.undecryptable {
  opacity: 0.6;
}

.meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.4rem;
  font-size: 0.75rem;
  color: var(--muted);
}

.nick {
  color: #dcdcdc;
  font-weight: 600;
}

.dot {
  display: inline-block;
  width: 0.6rem;
  height: 0.6rem;
  border-radius: 50%;
}

.text {
  margin: 0.2rem 0 0;
  white-space: pre-wrap;
  font-size: 0.95rem;
}

.text a,
.pinned a {
  color: #8db2ff;
}

.muted {
  color: var(--muted);
  font-style: italic;
}

.location {
  display: inline-block;
  margin-top: 0.25rem;
  font-size: 0.8rem;
  color: #8db2ff;
}

.mod-actions {
  margin-top: 0.2rem;
  text-align: right;
}

.chip {
  font-size: 0.68rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  padding: 0.05rem 0.4rem;
  border-radius: 4px;
  background: #444;
  color: #eee;
  font-weight: 700;
}

.chip-alert { background: #7a2e2e; color: #ffd0d0; }
.chip-need { background: #6a4a1e; color: #ffe0b0; }
.chip-offer { background: #245a33; color: #c8f5d4; }
.chip-info { background: #24456a; color: #cfe3ff; }

.composer {
  border-top: 1px solid var(--line);
  padding: 0.65rem 1rem 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.composer-options,
.composer-row {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
  align-items: stretch;
}

.composer-foot {
  margin: 0;
  display: flex;
  justify-content: space-between;
  gap: 0.5rem;
  font-size: 0.72rem;
  color: var(--muted);
}

.main-pane > .notice {
  margin: 0.75rem 1rem;
}

@media (max-width: 768px) {
  .messages-layout {
    grid-template-columns: 1fr;
  }

  /* Channel chips, then the conversation, then join form and nickname. */
  .sidebar {
    display: contents;
  }

  .channel-list { order: 0; }
  .main-pane { order: 1; }
  .join-details { order: 2; }
  .identity { order: 3; }

  .channel-list {
    flex-direction: row;
    overflow-x: auto;
    padding-bottom: 0.25rem;
  }

  .channel-list li {
    flex: 0 0 auto;
    max-width: 70vw;
  }

  .main-pane {
    max-height: none;
    min-height: 60vh;
  }

  .message {
    max-width: 92%;
  }
}
</style>
