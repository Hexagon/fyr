// Defensive wrapper around Web Storage.
//
// Every access is wrapped in try/catch: localStorage can be missing (tests,
// some embedded browsers), disabled by policy, throw SecurityError when site
// data is blocked, or throw QuotaExceededError on write. Callers always get a
// value back and never see an exception.

export const getDefaultStorage = () => {
  try {
    if (typeof window !== 'undefined' && window.localStorage) {
      return window.localStorage
    }
  } catch {
    // Accessing window.localStorage itself can throw (e.g. blocked site data).
  }
  return null
}

export const readJson = (storage, key, fallback = null) => {
  if (!storage) return fallback
  let raw
  try {
    raw = storage.getItem(key)
  } catch {
    return fallback
  }
  if (raw === null || raw === undefined || raw === '') return fallback
  try {
    return JSON.parse(raw)
  } catch {
    return fallback
  }
}

/** Returns true when the value was persisted. */
export const writeJson = (storage, key, value) => {
  if (!storage) return false
  let raw
  try {
    raw = JSON.stringify(value)
  } catch {
    return false
  }
  try {
    storage.setItem(key, raw)
    return true
  } catch {
    return false
  }
}

export const removeKey = (storage, key) => {
  if (!storage) return false
  try {
    storage.removeItem(key)
    return true
  } catch {
    return false
  }
}
