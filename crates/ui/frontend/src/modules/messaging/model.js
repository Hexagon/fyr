// Channel message model shared by the public board and encrypted channels.
// Server rows are converted to view messages here; encrypted rows are
// decrypted with the channel key and fall back to an "undecryptable"
// placeholder instead of throwing.

import { decryptMessage } from './crypto.js'

export const PUBLIC_CHANNEL_ID = 'public'
export const MAX_KEPT_MESSAGES = 1000

/**
 * @param {object} row server message row
 * @param {{ encKey?: Uint8Array, deviceTag?: string }} ctx
 */
export const toViewMessage = (row, ctx = {}) => {
  const base = {
    id: row.id,
    channelId: row.channel_id,
    created_at: row.created_at,
    expires_at: row.expires_at ?? null,
    pinned: Boolean(row.pinned)
  }
  let fields
  if (row.ciphertext != null) {
    try {
      const payload = decryptMessage(ctx.encKey, row.channel_id, row)
      fields = {
        nick: payload.nick,
        device_tag: payload.device_tag,
        text: payload.text,
        category: payload.category ?? null,
        lat: payload.lat ?? null,
        lng: payload.lng ?? null,
        sent_at: payload.sent_at,
        encrypted: true
      }
    } catch {
      fields = { nick: '?', device_tag: '0000', text: '', category: null, lat: null, lng: null, encrypted: true, undecryptable: true }
    }
  } else {
    fields = {
      nick: row.nick ?? '',
      device_tag: row.device_tag ?? '0000',
      text: row.text ?? '',
      category: row.category ?? null,
      lat: row.lat ?? null,
      lng: row.lng ?? null,
      encrypted: false
    }
  }
  const message = { ...base, ...fields }
  message.own = Boolean(ctx.deviceTag) && message.device_tag === ctx.deviceTag && !message.undecryptable
  return message
}

/** Merge by id (incoming wins), sorted ascending, keeping the newest `max`. */
export const mergeMessages = (current, incoming, max = MAX_KEPT_MESSAGES) => {
  const byId = new Map(current.map((m) => [m.id, m]))
  for (const message of incoming) byId.set(message.id, message)
  const merged = [...byId.values()].sort((a, b) => a.id - b.id)
  return merged.length > max ? merged.slice(merged.length - max) : merged
}

export const removeMessage = (current, id) => current.filter((m) => m.id !== id)

/** Drop expired, unpinned messages (the server prunes lazily). */
export const dropExpired = (current, now = Date.now()) =>
  current.filter((m) => m.pinned || m.expires_at == null || m.expires_at > now)

export const latestId = (messages) => messages.reduce((max, m) => (m.id > max ? m.id : max), 0)

/** Active alerts for the Home banner: alert category, unexpired, not dismissed. */
export const activeAlerts = (messages, dismissed = new Set(), now = Date.now()) =>
  dropExpired(messages, now).filter((m) => m.category === 'alert' && !dismissed.has(m.id))
