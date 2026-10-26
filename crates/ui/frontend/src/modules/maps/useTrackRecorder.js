// Vue composable wrapping navigator.geolocation.watchPosition for GPS track recording.
// The in-progress / unsaved track is snapshotted to localStorage so a reload or
// a crashed mobile tab does not lose it.

import { reactive, shallowRef } from 'vue'
import {
  createTrack,
  positionToFix,
  addFix,
  trackDurationMs,
  serializeTrack,
  deserializeTrack,
  checkGeolocationSupport,
  geolocationErrorMessage
} from './tracking.js'

export const TRACK_STORAGE_KEY = 'fyr.maps.trackRecording'
const PERSIST_INTERVAL_MS = 10_000

const storage = {
  read() {
    try { return globalThis.localStorage?.getItem(TRACK_STORAGE_KEY) ?? null } catch { return null }
  },
  write(value) {
    try { globalThis.localStorage?.setItem(TRACK_STORAGE_KEY, value) } catch { /* storage full/blocked */ }
  },
  clear() {
    try { globalThis.localStorage?.removeItem(TRACK_STORAGE_KEY) } catch { /* ignore */ }
  }
}

/**
 * Create a recorder. Listeners are attached with `setListeners` so a page can
 * subscribe while mounted; use `getTrackRecorder()` for the app-wide instance so
 * recording keeps running while the user visits other pages.
 * @param {object} [options]
 * @param {object} [options.env] injectable { navigator, isSecureContext } for tests
 */
export const createTrackRecorder = ({ env } = {}) => {
  let onChange = null
  let onFix = null
  const getNavigator = () => env?.navigator ?? globalThis.navigator
  const getSecure = () => (env && 'isSecureContext' in env ? env.isSecureContext : globalThis.isSecureContext)

  const state = reactive({
    recording: false,
    points: 0,
    distanceKm: 0,
    durationMs: 0,
    accuracy: null,
    error: null,
    warning: null,
    recovered: false
  })
  // The finished (stopped, unsaved) track; raw object kept out of deep reactivity.
  const finished = shallowRef(null)

  let track = null
  let watchId = null
  let ticker = null
  let lastPersist = 0
  let wakeLock = null

  const syncStats = (t) => {
    state.points = t ? t.points.length : 0
    state.distanceKm = t ? t.distanceKm : 0
    state.durationMs = t ? (state.recording ? trackDurationMs(t) : Math.max(0, (t.lastFixAt ?? t.startedAt) - t.startedAt)) : 0
  }

  const persist = (force = false) => {
    if (!track) return
    const now = Date.now()
    if (!force && now - lastPersist < PERSIST_INTERVAL_MS) return
    lastPersist = now
    storage.write(serializeTrack(track))
  }

  const requestWakeLock = async () => {
    try {
      const nav = getNavigator()
      if (nav?.wakeLock?.request) wakeLock = await nav.wakeLock.request('screen')
    } catch {
      wakeLock = null
    }
  }

  const releaseWakeLock = () => {
    try { wakeLock?.release?.() } catch { /* ignore */ }
    wakeLock = null
  }

  const onVisibility = () => {
    // Wake locks are released when the page is hidden; re-acquire on return.
    if (state.recording && globalThis.document?.visibilityState === 'visible') requestWakeLock()
  }

  const stopWatching = () => {
    const nav = getNavigator()
    if (watchId !== null && nav?.geolocation?.clearWatch) nav.geolocation.clearWatch(watchId)
    watchId = null
    if (ticker) clearInterval(ticker)
    ticker = null
    releaseWakeLock()
    globalThis.document?.removeEventListener?.('visibilitychange', onVisibility)
  }

  const handlePosition = (position) => {
    if (!track) return
    const fix = positionToFix(position)
    if (!fix) return
    state.warning = null
    state.accuracy = fix.accuracy
    onFix?.(fix)
    const { accepted, reason } = addFix(track, fix)
    if (reason === 'inaccurate') {
      state.warning = `Waiting for a better GPS fix (±${Math.round(fix.accuracy)} m)`
    }
    if (accepted) {
      syncStats(track)
      persist()
      onChange?.(track)
    }
  }

  const handleError = (err) => {
    if (err?.code === 1 || getSecure() === false) {
      state.error = geolocationErrorMessage(err, { isSecureContext: getSecure() })
      stop()
      return
    }
    // Temporary loss of signal or a timeout: keep watching.
    state.warning = geolocationErrorMessage(err, { isSecureContext: getSecure() })
  }

  /** Start recording. Returns false (and sets state.error) when geolocation cannot work. */
  const start = () => {
    if (state.recording) return true
    state.error = null
    state.warning = null
    const nav = getNavigator()
    const support = checkGeolocationSupport({ isSecureContext: getSecure(), geolocation: nav?.geolocation })
    if (!support.ok) {
      state.error = support.message
      return false
    }
    finished.value = null
    state.recovered = false
    track = createTrack(Date.now())
    state.recording = true
    syncStats(track)
    onChange?.(track)
    try {
      watchId = nav.geolocation.watchPosition(handlePosition, handleError, {
        enableHighAccuracy: true,
        maximumAge: 0,
        timeout: 30_000
      })
    } catch (err) {
      state.recording = false
      track = null
      state.error = geolocationErrorMessage(err, { isSecureContext: getSecure() })
      return false
    }
    ticker = setInterval(() => {
      if (track) state.durationMs = trackDurationMs(track)
    }, 1000)
    requestWakeLock()
    globalThis.document?.addEventListener?.('visibilitychange', onVisibility)
    persist(true)
    return true
  }

  /** Stop recording. The track (if ≥ 2 points) becomes `finished` until saved or discarded. */
  const stop = () => {
    const wasRecording = state.recording
    stopWatching()
    state.recording = false
    if (!track) return
    if (wasRecording) state.durationMs = trackDurationMs(track)
    if (track.points.length >= 2) {
      finished.value = track
      persist(true)
    } else {
      if (wasRecording && !state.error) state.warning = 'Track discarded: fewer than two GPS points were recorded.'
      track = null
      finished.value = null
      storage.clear()
      syncStats(null)
    }
    onChange?.(track)
  }

  /** Forget the finished track (after saving or when the user discards it). */
  const discard = () => {
    stopWatching()
    state.recording = false
    track = null
    finished.value = null
    state.recovered = false
    state.warning = null
    storage.clear()
    syncStats(null)
    onChange?.(null)
  }

  /** Restore an unsaved track left in localStorage by a previous session. */
  const recover = () => {
    const restored = deserializeTrack(storage.read() || '')
    if (!restored) {
      storage.clear()
      return false
    }
    if (restored.points.length < 2) {
      storage.clear()
      return false
    }
    track = restored
    finished.value = restored
    state.recovered = true
    syncStats(restored)
    onChange?.(restored)
    return true
  }

  /** Current track object (recording or finished), or null. */
  const current = () => track

  /**
   * Subscribe to updates: onChange(track|null) after points are added or the
   * track is cleared, onFix(fix) for every usable fix. Pass {} to unsubscribe.
   */
  const setListeners = (listeners = {}) => {
    onChange = listeners.onChange || null
    onFix = listeners.onFix || null
  }

  return { state, finished, start, stop, discard, recover, current, setListeners }
}

let sharedRecorder = null

/** App-wide recorder: survives Maps page unmounts so SPA navigation does not stop a recording. */
export const getTrackRecorder = () => {
  if (!sharedRecorder) sharedRecorder = createTrackRecorder()
  return sharedRecorder
}
