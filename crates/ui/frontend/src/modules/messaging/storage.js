// Per-device persistence for Messages (nickname, device id, remembered
// channels, read markers, dismissed alerts). Everything is wrapped in
// try/catch: storage may be blocked (private mode) and the page must keep
// working with in-memory values. Passphrases are NEVER stored.

const PREFIX = 'fyr.messages.'
const MAX_DISMISSED = 200
const MAX_CHANNELS = 50

const memoryStorage = () => {
  const map = new Map()
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, String(v)),
    removeItem: (k) => map.delete(k)
  }
}

const defaultStorage = () => {
  try {
    return globalThis.localStorage || null
  } catch {
    return null
  }
}

const randomHex = (bytes) => {
  const out = new Uint8Array(bytes)
  globalThis.crypto.getRandomValues(out)
  return Array.from(out, (b) => b.toString(16).padStart(2, '0')).join('')
}

const HEX64 = /^[0-9a-f]{64}$/
const HEX32 = /^[0-9a-f]{32}$/

export const createMessagingStorage = (backing = defaultStorage()) => {
  const fallback = memoryStorage()
  const read = (key) => {
    try {
      const value = (backing || fallback).getItem(PREFIX + key)
      return value ?? fallback.getItem(PREFIX + key)
    } catch {
      return fallback.getItem(PREFIX + key)
    }
  }
  const write = (key, value) => {
    fallback.setItem(PREFIX + key, value)
    try {
      backing?.setItem(PREFIX + key, value)
    } catch {
      // Quota exceeded or storage blocked: in-memory copy still works.
    }
  }
  const remove = (key) => {
    fallback.removeItem(PREFIX + key)
    try {
      backing?.removeItem(PREFIX + key)
    } catch {
      // ignore
    }
  }
  const readJson = (key, empty) => {
    try {
      const raw = read(key)
      return raw ? JSON.parse(raw) : empty
    } catch {
      return empty
    }
  }

  const api = {
    /** Random per-device id; only a short prefix (the tag) leaves the device. */
    deviceId() {
      let id = read('device')
      if (!id || !/^[0-9a-f]{32}$/.test(id)) {
        id = randomHex(16)
        write('device', id)
      }
      return id
    },
    deviceTag() {
      return api.deviceId().slice(0, 6)
    },
    nickname() {
      return (read('nick') || '').slice(0, 40)
    },
    setNickname(nick) {
      write('nick', String(nick || '').trim().slice(0, 40))
    },

    /** Remembered channels: { name, channelId, authKey, encKey, version }. */
    channels() {
      const list = readJson('channels', [])
      return Array.isArray(list)
        ? list.filter((c) => c && typeof c.name === 'string' && HEX32.test(c.channelId) && HEX64.test(c.authKey) && HEX64.test(c.encKey))
        : []
    },
    rememberChannel({ name, channelId, authKey, encKey, version = 1 }) {
      const entry = { name: String(name), channelId, authKey, encKey, version }
      if (!HEX32.test(channelId) || !HEX64.test(authKey) || !HEX64.test(encKey)) throw new Error('Invalid channel keys')
      const rest = api.channels().filter((c) => c.channelId !== channelId)
      write('channels', JSON.stringify([...rest, entry].slice(-MAX_CHANNELS)))
      return entry
    },
    forgetChannel(channelId) {
      write('channels', JSON.stringify(api.channels().filter((c) => c.channelId !== channelId)))
      const seen = readJson('lastSeen', {})
      delete seen[channelId]
      write('lastSeen', JSON.stringify(seen))
    },

    lastSeen(channelId) {
      const value = readJson('lastSeen', {})[channelId]
      return Number.isSafeInteger(value) ? value : 0
    },
    setLastSeen(channelId, id) {
      const seen = readJson('lastSeen', {})
      if (!Number.isSafeInteger(id) || id <= (seen[channelId] || 0)) return
      seen[channelId] = id
      write('lastSeen', JSON.stringify(seen))
    },

    dismissedAlerts() {
      const list = readJson('dismissedAlerts', [])
      return new Set(Array.isArray(list) ? list.filter(Number.isSafeInteger) : [])
    },
    dismissAlert(id) {
      const list = [...api.dismissedAlerts(), id].slice(-MAX_DISMISSED)
      write('dismissedAlerts', JSON.stringify(list))
    },

    /** Test/maintenance helper. */
    clearAll() {
      for (const key of ['device', 'nick', 'channels', 'lastSeen', 'dismissedAlerts']) remove(key)
    }
  }
  return api
}

let shared = null
export const messagingStorage = () => {
  if (!shared) shared = createMessagingStorage()
  return shared
}
