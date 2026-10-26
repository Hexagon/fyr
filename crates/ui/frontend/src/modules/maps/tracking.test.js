import { describe, expect, it } from 'vitest'
import {
  createTrack,
  positionToFix,
  addFix,
  trackDurationMs,
  defaultTrackName,
  trackToFeature,
  trackToGeoJSON,
  serializeTrack,
  deserializeTrack,
  checkGeolocationSupport,
  geolocationErrorMessage,
  INSECURE_CONTEXT_MESSAGE
} from './tracking.js'

const T0 = Date.UTC(2026, 6, 1, 8, 0, 0)
const pos = (lat, lng, { t = T0, accuracy = 5, altitude = null } = {}) => ({
  coords: { latitude: lat, longitude: lng, accuracy, altitude },
  timestamp: t
})

describe('positionToFix', () => {
  it('converts GeolocationPosition-like objects', () => {
    expect(positionToFix(pos(59.33, 18.07, { altitude: 25 }))).toEqual({ lat: 59.33, lng: 18.07, alt: 25, accuracy: 5, time: T0 })
    expect(positionToFix(pos(-33.9, -151.2)).alt).toBeNull()
  })

  it('rejects unusable input', () => {
    expect(positionToFix(null)).toBeNull()
    expect(positionToFix({ coords: { latitude: 'x', longitude: 1 } })).toBeNull()
    expect(positionToFix({ coords: { latitude: 100, longitude: 1 } })).toBeNull()
  })
})

describe('addFix', () => {
  it('accumulates points, times and distance', () => {
    const track = createTrack(T0)
    expect(addFix(track, positionToFix(pos(0, 0)))).toEqual({ accepted: true, reason: null })
    expect(addFix(track, positionToFix(pos(0.001, 0, { t: T0 + 10_000 }))).accepted).toBe(true)
    expect(track.points).toEqual([[0, 0], [0, 0.001]])
    expect(track.times).toEqual([new Date(T0).toISOString(), new Date(T0 + 10_000).toISOString()])
    expect(track.distanceKm).toBeCloseTo(0.111, 3)
  })

  it('keeps altitude as the third coordinate', () => {
    const track = createTrack(T0)
    addFix(track, positionToFix(pos(1, 2, { altitude: 300 })))
    expect(track.points[0]).toEqual([2, 1, 300])
  })

  it('drops inaccurate, jittery, stale and invalid fixes', () => {
    const track = createTrack(T0)
    addFix(track, positionToFix(pos(0, 0)))
    expect(addFix(track, positionToFix(pos(0.01, 0, { accuracy: 500 }))).reason).toBe('inaccurate')
    expect(addFix(track, positionToFix(pos(0.00001, 0, { t: T0 + 1000 }))).reason).toBe('jitter')
    expect(addFix(track, positionToFix(pos(0.01, 0, { t: T0 - 1000 }))).reason).toBe('stale')
    expect(addFix(track, null).reason).toBe('invalid')
    expect(track.points).toHaveLength(1)
    expect(track.distanceKm).toBe(0)
  })

  it('respects custom thresholds', () => {
    const track = createTrack(T0)
    addFix(track, positionToFix(pos(0, 0)))
    expect(addFix(track, positionToFix(pos(0.00001, 0, { t: T0 + 1 })), { minStepM: 0 }).accepted).toBe(true)
    expect(addFix(track, positionToFix(pos(0.01, 0, { t: T0 + 2, accuracy: 500 })), { maxAccuracyM: 1000 }).accepted).toBe(true)
  })
})

describe('track output', () => {
  const build = () => {
    const track = createTrack(T0)
    addFix(track, positionToFix(pos(59.33, 18.07, { altitude: 10 })))
    addFix(track, positionToFix(pos(59.34, 18.08, { t: T0 + 60_000, altitude: 12 })))
    return track
  }

  it('computes duration', () => {
    expect(trackDurationMs(createTrack(T0), T0 + 5000)).toBe(5000)
    expect(trackDurationMs(createTrack(T0), T0 - 5000)).toBe(0)
    expect(trackDurationMs(null)).toBe(0)
  })

  it('names tracks by local start time', () => {
    expect(defaultTrackName(new Date(2026, 9, 25, 14, 3))).toBe('Track 2026-10-25 14:03')
  })

  it('builds a LineString feature with timestamps', () => {
    const feature = trackToFeature(build(), 'Morning walk')
    expect(feature.geometry).toEqual({ type: 'LineString', coordinates: [[18.07, 59.33, 10], [18.08, 59.34, 12]] })
    expect(feature.properties).toMatchObject({
      name: 'Morning walk',
      gpxType: 'trk',
      time: new Date(T0).toISOString(),
      endTime: new Date(T0 + 60_000).toISOString(),
      coordTimes: [new Date(T0).toISOString(), new Date(T0 + 60_000).toISOString()]
    })
    expect(feature.properties.distanceKm).toBeGreaterThan(1)
    expect(trackToFeature(build()).properties.name).toMatch(/^Track \d{4}-\d{2}-\d{2} \d{2}:\d{2}$/)
  })

  it('returns null / empty output for tracks with fewer than two points', () => {
    expect(trackToFeature(createTrack(T0))).toBeNull()
    expect(trackToGeoJSON(createTrack(T0)).features).toEqual([])
    expect(trackToGeoJSON(build()).features).toHaveLength(1)
  })

  it('serialises for recovery and validates on restore', () => {
    const track = build()
    const restored = deserializeTrack(serializeTrack(track))
    expect(restored.points).toEqual(track.points)
    expect(restored.times).toEqual(track.times)
    expect(restored.distanceKm).toBeCloseTo(track.distanceKm)
    expect(restored.startedAt).toBe(T0)
    expect(deserializeTrack('nope')).toBeNull()
    expect(deserializeTrack('{"v":2}')).toBeNull()
    expect(deserializeTrack(JSON.stringify({ v: 1, points: [[1, 2]], times: [] }))).toBeNull()
    expect(deserializeTrack(JSON.stringify({ v: 1, points: [['a', 2]], times: ['t'] }))).toBeNull()
  })
})

describe('geolocation support and errors', () => {
  it('flags insecure contexts first', () => {
    expect(checkGeolocationSupport({ isSecureContext: false, geolocation: {} })).toEqual({ ok: false, message: INSECURE_CONTEXT_MESSAGE })
    expect(checkGeolocationSupport({ isSecureContext: true }).ok).toBe(false)
    expect(checkGeolocationSupport({ isSecureContext: true, geolocation: {} }).ok).toBe(true)
    expect(checkGeolocationSupport({ geolocation: {} }).ok).toBe(true)
  })

  it('maps error codes to messages', () => {
    expect(geolocationErrorMessage({ code: 1 })).toMatch(/denied/)
    expect(geolocationErrorMessage({ code: 2 })).toMatch(/no GPS fix/)
    expect(geolocationErrorMessage({ code: 3 })).toMatch(/Timed out/)
    expect(geolocationErrorMessage({})).toBe('Could not determine location')
    expect(geolocationErrorMessage({ code: 1 }, { isSecureContext: false })).toBe(INSECURE_CONTEXT_MESSAGE)
  })
})
