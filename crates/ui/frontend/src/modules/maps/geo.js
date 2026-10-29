// Pure geographic helpers. Positions are GeoJSON-style [lng, lat] arrays.

import { formatBytes as formatLocaleBytes, formatNumber } from '../../i18n/index.js'

export const EARTH_RADIUS_KM = 6371

const toRad = (x) => (x * Math.PI) / 180

/** Great-circle (haversine) distance between two [lng, lat] points in km. */
export const haversineDistance = (a, b) => {
  const dLat = toRad(b[1] - a[1])
  const dLon = toRad(b[0] - a[0])
  const sinHalfLat = Math.sin(dLat / 2)
  const sinHalfLon = Math.sin(dLon / 2)
  const h = sinHalfLat * sinHalfLat + Math.cos(toRad(a[1])) * Math.cos(toRad(b[1])) * sinHalfLon * sinHalfLon
  // Clamp: floating point error can push h marginally above 1 for antipodes.
  return 2 * EARTH_RADIUS_KM * Math.asin(Math.sqrt(Math.min(1, h)))
}

/** Total length in km of a path of [lng, lat] points. */
export const pathLength = (points) => {
  if (!Array.isArray(points)) return 0
  let total = 0
  for (let i = 1; i < points.length; i++) {
    total += haversineDistance(points[i - 1], points[i])
  }
  return total
}

/** Human readable distance: metres below 1 km, otherwise km with 2 decimals (UI locale). */
export const formatDistance = (km) => {
  if (km < 1) return `${formatNumber(Math.round(km * 1000), { useGrouping: false })} m`
  return `${formatNumber(km, { minimumFractionDigits: 2, maximumFractionDigits: 2, useGrouping: false })} km`
}

/** Human readable duration for a number of milliseconds: "1:02:03" or "2:03". */
export const formatDuration = (ms) => {
  const totalSeconds = Math.max(0, Math.floor((Number(ms) || 0) / 1000))
  const hours = Math.floor(totalSeconds / 3600)
  const minutes = Math.floor((totalSeconds % 3600) / 60)
  const seconds = totalSeconds % 60
  const pad = (n) => String(n).padStart(2, '0')
  return hours > 0 ? `${hours}:${pad(minutes)}:${pad(seconds)}` : `${minutes}:${pad(seconds)}`
}

export const formatBytes = (bytes) => formatLocaleBytes(bytes)

/** Wrap a longitude into [-180, 180]. */
export const normalizeLng = (lng) => {
  const n = Number(lng)
  if (!Number.isFinite(n)) return NaN
  if (n >= -180 && n <= 180) return n
  return ((((n + 180) % 360) + 360) % 360) - 180
}

export const isValidLatLng = (lat, lng) =>
  Number.isFinite(lat) && Number.isFinite(lng) && lat >= -90 && lat <= 90 && lng >= -180 && lng <= 180

// ── Bounds ([west, south, east, north]) ────────────────────────────

/**
 * Normalise a bounds value into [west, south, east, north] or null.
 * Accepts arrays and MBTiles-style "w,s,e,n" strings. Degenerate (zero-area)
 * or out-of-range bounds are rejected.
 */
export const normalizeBounds = (value) => {
  let parts = value
  if (typeof value === 'string') parts = value.split(',')
  if (!Array.isArray(parts) || parts.length !== 4) return null
  const [w, s, e, n] = parts.map((p) => (typeof p === 'string' && p.trim() === '' ? NaN : Number(p)))
  if (![w, s, e, n].every(Number.isFinite)) return null
  if (s < -90 || n > 90 || s >= n) return null
  if (w < -180 || w > 180 || e < -180 || e > 180 || w === e) return null
  return [w, s, e, n]
}

/** Bounds from a PMTiles v3 header (minLon/minLat/maxLon/maxLat). */
export const boundsFromPmtilesHeader = (header) => {
  if (!header) return null
  return normalizeBounds([header.minLon, header.minLat, header.maxLon, header.maxLat])
}

/** True when bounds contain the point. Handles bounds crossing the antimeridian (west > east). */
export const boundsContain = (bounds, lng, lat) => {
  const b = normalizeBounds(bounds)
  if (!b || !Number.isFinite(lng) || !Number.isFinite(lat)) return false
  const [w, s, e, n] = b
  if (lat < s || lat > n) return false
  const x = normalizeLng(lng)
  return w <= e ? x >= w && x <= e : x >= w || x <= e
}

/** Approximate bounds area in square degrees (used to prefer the most detailed map). */
export const boundsArea = (bounds) => {
  const b = normalizeBounds(bounds)
  if (!b) return Infinity
  const [w, s, e, n] = b
  const width = w <= e ? e - w : 360 - (w - e)
  return width * (n - s)
}

export const boundsCenter = (bounds) => {
  const b = normalizeBounds(bounds)
  if (!b) return null
  const [w, s, e, n] = b
  const width = w <= e ? e - w : 360 - (w - e)
  return [normalizeLng(w + width / 2), (s + n) / 2]
}

/**
 * Pick the map whose bounds contain the point. When several do, the one with
 * the smallest bounds (usually the most detailed regional extract) wins.
 * `entries` is an array of `{ map, bounds }`. Returns the map or null.
 */
export const pickMapForPoint = (entries, lng, lat) => {
  let best = null
  let bestArea = Infinity
  for (const entry of entries || []) {
    if (!entry || !boundsContain(entry.bounds, lng, lat)) continue
    const area = boundsArea(entry.bounds)
    if (area < bestArea) {
      best = entry.map
      bestArea = area
    }
  }
  return best
}
