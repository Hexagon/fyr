// Shared live feed of the public channel ("Board").
//
// One SSE connection per tab serves both the nav unread badge (App.vue) and
// the Board view in Messages.vue, keeping us well below the browser's
// per-origin connection limit on HTTP/1.1.

import { reactive, readonly } from 'vue'
import { apiService } from '../../services/api.js'
import { createChannelStream } from './stream.js'
import { messagingStorage } from './storage.js'
import { PUBLIC_CHANNEL_ID } from './model.js'

const state = reactive({
  enabled: false,
  status: 'stopped',
  unread: 0,
  viewing: false
})

const listeners = new Set()
let stream = null
let starting = null

const storage = () => messagingStorage()

const isUnreadCandidate = (row) =>
  row && Number.isSafeInteger(row.id) && row.id > storage().lastSeen(PUBLIC_CHANNEL_ID) && !storage().ownIds(PUBLIC_CHANNEL_ID).has(row.id)

const recountUnread = async () => {
  if (state.viewing) {
    state.unread = 0
    return
  }
  const since = storage().lastSeen(PUBLIC_CHANNEL_ID)
  const data = await apiService.listChannelMessages(PUBLIC_CHANNEL_ID, since ? { since, limit: 100 } : { limit: 20 })
  state.unread = (data.messages || []).filter(isUnreadCandidate).length
}

export const useBoardFeed = () => readonly(state)

/** Start the shared feed (idempotent). Resolves once config is known. */
export const startBoardFeed = () => {
  if (starting) return starting
  starting = (async () => {
    let config
    try {
      config = await apiService.getMessagesConfig()
    } catch {
      starting = null
      return
    }
    state.enabled = Boolean(config?.enabled)
    if (!state.enabled) return
    stream = createChannelStream({
      url: apiService.channelStreamUrl(PUBLIC_CHANNEL_ID),
      onEvent: (name, data) => {
        if (name === 'message' && !state.viewing && isUnreadCandidate(data)) state.unread += 1
        for (const listener of listeners) listener.onEvent?.(name, data)
      },
      onStatus: (status, detail) => {
        state.status = status
        for (const listener of listeners) listener.onStatus?.(status, detail)
      },
      onCatchUp: async () => {
        await Promise.allSettled([recountUnread(), ...[...listeners].map((l) => l.onCatchUp?.())])
      }
    })
    stream.start()
  })()
  return starting
}

/** Subscribe to board events: { onEvent, onStatus, onCatchUp }. Returns unsubscribe. */
export const subscribeBoard = (listener) => {
  listeners.add(listener)
  listener.onStatus?.(state.status)
  return () => listeners.delete(listener)
}

export const setBoardViewing = (viewing) => {
  state.viewing = viewing
  if (viewing) state.unread = 0
}

export const markBoardSeen = (id) => {
  storage().setLastSeen(PUBLIC_CHANNEL_ID, id)
  if (state.viewing) state.unread = 0
}

export const reconnectBoard = () => stream?.kick()
