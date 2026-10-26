// Pure track-recording logic used with navigator.geolocation.watchPosition.

import { haversineDistance } from './geo.js'

/** Fixes worse than this (metres, 1σ) are ignored. */
export const DEFAULT_MAX_ACCURACY_M = 75
/** Moves shorter than this (metres) are treated as GPS jitter and not recorded. */
export const DEFAULT_MIN_STEP_M = 3

export const createTrack = (startedAt = Date.now()) => ({
  points: [], // [lng, lat] or [lng, lat, alt]
  times: [], // ISO strings, one per point
  distanceKm: 0,
  startedAt,
  lastFixAt: null,
  lastAccuracy: null
})

/**
 * Convert a GeolocationPosition (or a plain {coords, timestamp}) to a fix.
 * Returns null for unusable input.
 */
export const positionToFix = (position) => {
  const c = position?.coords
  if (!c) return null
  const lat = Number(c.latitude)
  const lng = Number(c.longitude)
  if (!Number.isFinite(lat) || !Number.isFinite(lng) || Math.abs(lat) > 90 || Math.abs(lng) > 180) return null
  const alt = c.altitude === null || c.altitude === undefined ? NaN : Number(c.altitude)
  const accuracy = c.accuracy === null || c.accuracy === undefined ? NaN : Number(c.accuracy)
  const time = Number.isFinite(Number(position.timestamp)) ? Number(position.timestamp) : Date.now()
  return {
    lat,
    lng,
    alt: Number.isFinite(alt) ? alt : null,
    accuracy: Number.isFinite(accuracy) ? accuracy : null,
    time
  }
}

/**
 * Add a fix to a track (mutates `track`). Returns { accepted, reason }.
 * Reasons: 'invalid', 'inaccurate', 'stale', 'jitter'.
 */
export const addFix = (track, fix, { maxAccuracyM = DEFAULT_MAX_ACCURACY_M, minStepM = DEFAULT_MIN_STEP_M } = {}) => {
  if (!track || !fix || !Number.isFinite(fix.lat) || !Number.isFinite(fix.lng)) {
    return { accepted: false, reason: 'invalid' }
  }
  track.lastAccuracy = fix.accuracy
  if (fix.accuracy !== null && fix.accuracy !== undefined && fix.accuracy > maxAccuracyM) {
    return { accepted: false, reason: 'inaccurate' }
  }
  if (track.lastFixAt !== null && fix.time < track.lastFixAt) {
    return { accepted: false, reason: 'stale' }
  }
  const position = fix.alt !== null && fix.alt !== undefined ? [fix.lng, fix.lat, fix.alt] : [fix.lng, fix.lat]
  const last = track.points[track.points.length - 1]
  if (last) {
    const stepKm = haversineDistance(last, position)
    if (stepKm * 1000 < minStepM) {
      track.lastFixAt = fix.time
      return { accepted: false, reason: 'jitter' }
    }
    track.distanceKm += stepKm
  }
  track.points.push(position)
  track.times.push(new Date(fix.time).toISOString())
  track.lastFixAt = fix.time
  return { accepted: true, reason: null }
}

export const trackDurationMs = (track, now = Date.now()) => {
  if (!track) return 0
  return Math.max(0, now - track.startedAt)
}

/** Default track name, e.g. "Track 2026-10-25 14:03" (local time). */
export const defaultTrackName = (date = new Date()) => {
  const d = date instanceof Date ? date : new Date(date)
  const pad = (n) => String(n).padStart(2, '0')
  return `Track ${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** LineString feature for a recorded track, or null when it has fewer than two points. */
export const trackToFeature = (track, name) => {
  if (!track || track.points.length < 2) return null
  return {
    type: 'Feature',
    geometry: { type: 'LineString', coordinates: track.points.map((p) => [...p]) },
    properties: {
      name: name || defaultTrackName(new Date(track.startedAt)),
      gpxType: 'trk',
      source: 'fyr-track-recording',
      time: track.times[0],
      endTime: track.times[track.times.length - 1],
      distanceKm: Math.round(track.distanceKm * 1000) / 1000,
      coordTimes: [...track.times]
    }
  }
}

/** Live GeoJSON for the in-progress track. */
export const trackToGeoJSON = (track) => ({
  type: 'FeatureCollection',
  features: track && track.points.length >= 2
    ? [{ type: 'Feature', geometry: { type: 'LineString', coordinates: track.points }, properties: {} }]
    : []
})

/** Serialisable snapshot for crash/reload recovery (localStorage). */
export const serializeTrack = (track) => JSON.stringify({
  v: 1,
  points: track.points,
  times: track.times,
  distanceKm: track.distanceKm,
  startedAt: track.startedAt,
  lastFixAt: track.lastFixAt
})

export const deserializeTrack = (text) => {
  try {
    const data = JSON.parse(text)
    if (!data || data.v !== 1 || !Array.isArray(data.points) || !Array.isArray(data.times)) return null
    if (data.points.length !== data.times.length) return null
    if (!data.points.every((p) => Array.isArray(p) && p.length >= 2 && p.every(Number.isFinite))) return null
    return {
      points: data.points,
      times: data.times.map(String),
      distanceKm: Number(data.distanceKm) || 0,
      startedAt: Number(data.startedAt) || Date.now(),
      lastFixAt: Number.isFinite(data.lastFixAt) ? data.lastFixAt : null,
      lastAccuracy: null
    }
  } catch {
    return null
  }
}

// ── Geolocation availability / errors ──────────────────────────────

export const INSECURE_CONTEXT_MESSAGE =
  'Location needs a secure connection: open Fyr via https:// or http://localhost. Browsers block GPS on plain http:// LAN addresses; the server operator can turn on HTTPS (FYR_TLS=self-signed).'

/**
 * Check whether geolocation can work here.
 * @returns {{ ok: boolean, message: string|null }}
 */
export const checkGeolocationSupport = ({ isSecureContext, geolocation } = {}) => {
  if (isSecureContext === false) return { ok: false, message: INSECURE_CONTEXT_MESSAGE }
  if (!geolocation) return { ok: false, message: 'Geolocation is not supported by this browser' }
  return { ok: true, message: null }
}

/** Human readable message for a GeolocationPositionError. */
export const geolocationErrorMessage = (err, { isSecureContext } = {}) => {
  if (isSecureContext === false) return INSECURE_CONTEXT_MESSAGE
  switch (err?.code) {
    case 1:
      return 'Location access was denied. Allow location for this site in your browser settings.'
    case 2:
      return 'Could not determine location (no GPS fix). Move to open sky and try again.'
    case 3:
      return 'Timed out waiting for a location fix.'
    default:
      return 'Could not determine location'
  }
}
