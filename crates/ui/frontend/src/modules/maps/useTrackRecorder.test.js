import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createTrackRecorder, TRACK_STORAGE_KEY } from './useTrackRecorder.js'
import { INSECURE_CONTEXT_MESSAGE } from './tracking.js'

const makeGeo = () => {
  const geo = {
    success: null,
    error: null,
    cleared: [],
    watchPosition: vi.fn((ok, err) => { geo.success = ok; geo.error = err; return 42 }),
    clearWatch: vi.fn((id) => geo.cleared.push(id))
  }
  return geo
}

const fix = (lat, lng, t, accuracy = 5) => ({ coords: { latitude: lat, longitude: lng, accuracy, altitude: null }, timestamp: t })

describe('createTrackRecorder', () => {
  beforeEach(() => {
    localStorage.clear()
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-07-01T08:00:00Z'))
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('refuses to start in an insecure context', () => {
    const geo = makeGeo()
    const rec = createTrackRecorder({ env: { navigator: { geolocation: geo }, isSecureContext: false } })
    expect(rec.start()).toBe(false)
    expect(rec.state.error).toBe(INSECURE_CONTEXT_MESSAGE)
    expect(geo.watchPosition).not.toHaveBeenCalled()
  })

  it('refuses to start without geolocation', () => {
    const rec = createTrackRecorder({ env: { navigator: {}, isSecureContext: true } })
    expect(rec.start()).toBe(false)
    expect(rec.state.error).toMatch(/not supported/)
  })

  it('records fixes, reports stats and keeps the finished track', () => {
    const geo = makeGeo()
    const onChange = vi.fn()
    const rec = createTrackRecorder({ env: { navigator: { geolocation: geo }, isSecureContext: true } })
    rec.setListeners({ onChange })
    expect(rec.start()).toBe(true)
    expect(rec.state.recording).toBe(true)
    const t0 = Date.now()
    geo.success(fix(0, 0, t0))
    geo.success(fix(0.001, 0, t0 + 5000))
    expect(rec.state.points).toBe(2)
    expect(rec.state.distanceKm).toBeCloseTo(0.111, 2)
    vi.advanceTimersByTime(3000)
    expect(rec.state.durationMs).toBeGreaterThanOrEqual(3000)
    rec.stop()
    expect(geo.cleared).toEqual([42])
    expect(rec.state.recording).toBe(false)
    expect(rec.finished.value.points).toHaveLength(2)
    expect(onChange).toHaveBeenCalled()
    expect(localStorage.getItem(TRACK_STORAGE_KEY)).toContain('"v":1')
  })

  it('stops with a clear message when permission is denied', () => {
    const geo = makeGeo()
    const rec = createTrackRecorder({ env: { navigator: { geolocation: geo }, isSecureContext: true } })
    rec.start()
    geo.error({ code: 1 })
    expect(rec.state.recording).toBe(false)
    expect(rec.state.error).toMatch(/denied/)
    expect(rec.finished.value).toBeNull()
  })

  it('keeps recording through temporary signal loss', () => {
    const geo = makeGeo()
    const rec = createTrackRecorder({ env: { navigator: { geolocation: geo }, isSecureContext: true } })
    rec.start()
    geo.error({ code: 3 })
    expect(rec.state.recording).toBe(true)
    expect(rec.state.warning).toMatch(/Timed out/)
    geo.success(fix(1, 1, Date.now(), 500))
    expect(rec.state.warning).toMatch(/better GPS fix/)
  })

  it('discards tracks that are too short', () => {
    const geo = makeGeo()
    const rec = createTrackRecorder({ env: { navigator: { geolocation: geo }, isSecureContext: true } })
    rec.start()
    geo.success(fix(0, 0, Date.now()))
    rec.stop()
    expect(rec.finished.value).toBeNull()
    expect(rec.state.warning).toMatch(/fewer than two/)
    expect(localStorage.getItem(TRACK_STORAGE_KEY)).toBeNull()
  })

  it('recovers an unsaved track from storage and discards it on request', () => {
    const geo = makeGeo()
    const first = createTrackRecorder({ env: { navigator: { geolocation: geo }, isSecureContext: true } })
    first.start()
    geo.success(fix(0, 0, Date.now()))
    geo.success(fix(0.01, 0, Date.now() + 1000))
    first.stop()

    const second = createTrackRecorder({ env: { navigator: { geolocation: makeGeo() }, isSecureContext: true } })
    expect(second.recover()).toBe(true)
    expect(second.state.recovered).toBe(true)
    expect(second.finished.value.points).toHaveLength(2)
    second.discard()
    expect(second.finished.value).toBeNull()
    expect(localStorage.getItem(TRACK_STORAGE_KEY)).toBeNull()
    expect(second.recover()).toBe(false)
  })
})
