<template>
  <div v-if="alerts.length" class="alert-banners" role="region" aria-label="Active alerts">
    <div v-for="alert in alerts" :key="alert.id" class="alert-banner" role="alert">
      <span class="alert-chip">Alert</span>
      <div class="alert-body">
        <p class="alert-text">{{ firstLine(alert.text, 160) }}</p>
        <p class="alert-meta">
          {{ alert.nick }} · {{ timeLabel(alert.created_at) }}
          <router-link v-if="alert.lat != null && alert.lng != null" :to="mapsLink({ lat: alert.lat, lng: alert.lng, label: alert.text })">📍 Map</router-link>
          <router-link to="/messages">Open Board</router-link>
        </p>
      </div>
      <button type="button" class="alert-dismiss" aria-label="Dismiss alert" @click="dismiss(alert.id)">✕</button>
    </div>
  </div>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { apiService } from '../../services/api'
import { firstLine, mapsLink, timeLabel } from './format.js'
import { PUBLIC_CHANNEL_ID, activeAlerts, mergeMessages, removeMessage, toViewMessage } from './model.js'
import { messagingStorage } from './storage.js'
import { startBoardFeed, subscribeBoard } from './boardFeed.js'

const storage = messagingStorage()
const rows = ref([])
const dismissed = ref(storage.dismissedAlerts())
const now = ref(Date.now())
let unsubscribe = null
let clock = null

const alerts = computed(() => activeAlerts(rows.value, dismissed.value, now.value).slice(-3).reverse())

const load = async () => {
  const data = await apiService.listChannelMessages(PUBLIC_CHANNEL_ID, { category: 'alert', limit: 10 })
  const view = [...(data.messages || []), ...(data.pinned || []).filter((m) => m.category === 'alert')].map((r) => toViewMessage(r))
  rows.value = mergeMessages([], view)
}

const dismiss = (id) => {
  storage.dismissAlert(id)
  dismissed.value = storage.dismissedAlerts()
}

onMounted(async () => {
  await startBoardFeed()
  try {
    await load()
  } catch {
    return // Messages disabled or server unreachable: show nothing.
  }
  unsubscribe = subscribeBoard({
    onEvent: (name, data) => {
      if ((name === 'message' || name === 'pinned') && data?.category === 'alert') rows.value = mergeMessages(rows.value, [toViewMessage(data)])
      if (name === 'deleted') rows.value = removeMessage(rows.value, data?.id)
    },
    onCatchUp: () => load().catch(() => {})
  })
  clock = window.setInterval(() => { now.value = Date.now() }, 60000)
})

onBeforeUnmount(() => {
  unsubscribe?.()
  if (clock) window.clearInterval(clock)
})
</script>

<style scoped>
.alert-banners {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.alert-banner {
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
  background: #3d2a2a;
  border-left: 4px solid #e05d5d;
  border-radius: 8px;
  padding: 0.75rem 1rem;
  color: #ffd9d9;
}

.alert-chip {
  font-size: 0.7rem;
  font-weight: 700;
  text-transform: uppercase;
  background: #7a2e2e;
  padding: 0.1rem 0.45rem;
  border-radius: 4px;
  flex: 0 0 auto;
}

.alert-body {
  flex: 1;
  min-width: 0;
}

.alert-text {
  margin: 0;
  word-break: break-word;
}

.alert-meta {
  margin: 0.25rem 0 0;
  font-size: 0.78rem;
  color: #d0a0a0;
  display: flex;
  gap: 0.6rem;
  flex-wrap: wrap;
}

.alert-meta a {
  color: #ffc0c0;
}

.alert-dismiss {
  background: none;
  border: none;
  color: #ffc0c0;
  cursor: pointer;
  font-size: 1rem;
}
</style>
