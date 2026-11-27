// Maps deep-link contract (shared with other pages/agents):
//
//   /maps?map=<filename>&lat=<lat>&lng=<lng>&zoom=<z>&label=<text>
//
// All parameters are optional.
// - map:   filename of a map under /data/maps (as listed by /api/content/maps).
// - lat/lng: WGS84 decimal degrees; both are required for a point. A labelled
//            marker is shown there and the view is centred on it.
// - zoom:  0–22 (defaults to DEFAULT_POINT_ZOOM when a point is given).
// - label: marker label (plain text, max MAX_LABEL_LENGTH chars).
// When `map` is absent but a point is given, the Maps page picks an available
// map whose bounds contain the point, else keeps the current/default map.
// `lon` and `long` are accepted as aliases for `lng`, and `z` for `zoom`, when
// parsing; longitudes outside [-180, 180] are wrapped.

import { normalizeLng } from './geo.js'

export const MAPS_PATH = '/maps'
export const DEFAULT_POINT_ZOOM = 14
export const MIN_ZOOM = 0
export const MAX_ZOOM = 22
export const MAX_LABEL_LENGTH = 200

const first = (value) => (Array.isArray(value) ? value[0] : value)

const toNumber = (value) => {
  const v = first(value)
  if (v === undefined || v === null) return NaN
  const s = String(v).trim()
  if (s === '') return NaN
  return Number(s)
}

const roundTo = (value, digits) => {
  const f = Math.pow(10, digits)
  return Math.round(value * f) / f
}

/** Strip control characters and clamp length of a user/URL supplied label. */
export const sanitizeLabel = (value) => {
  const v = first(value)
  if (v === undefined || v === null) return ''
  return String(v)
    .replace(/[\u0000-\u001f\u007f]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, MAX_LABEL_LENGTH)
}

/**
 * Parse a vue-router query object (values may be strings or arrays).
 * Returns { map, lat, lng, zoom, label, hasPoint } with nulls for absent/invalid values.
 */
export const parseMapsQuery = (query = {}) => {
  const q = query || {}
  const mapRaw = first(q.map)
  const map = typeof mapRaw === 'string' && mapRaw.trim() ? mapRaw.trim() : null

  const lat = toNumber(q.lat)
  const lngRaw = toNumber(q.lng ?? q.lon ?? q.long)
  const lng = normalizeLng(lngRaw)
  const hasPoint = Number.isFinite(lat) && Number.isFinite(lng) && lat >= -90 && lat <= 90

  const zoomRaw = toNumber(q.zoom ?? q.z)
  const zoom = Number.isFinite(zoomRaw) ? Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoomRaw)) : null

  const label = sanitizeLabel(q.label)

  return {
    map,
    lat: hasPoint ? lat : null,
    lng: hasPoint ? lng : null,
    zoom,
    label: label || null,
    hasPoint
  }
}

/** Build a query object for router.push/replace. Absent values are omitted. */
export const buildMapsQuery = ({ map, lat, lng, zoom, label } = {}) => {
  const query = {}
  if (map) query.map = String(map)
  if (Number.isFinite(lat) && Number.isFinite(lng)) {
    query.lat = String(roundTo(lat, 6))
    query.lng = String(roundTo(normalizeLng(lng), 6))
  }
  if (Number.isFinite(zoom)) query.zoom = String(roundTo(Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom)), 2))
  const cleanLabel = sanitizeLabel(label)
  if (cleanLabel && query.lat !== undefined) query.label = cleanLabel
  return query
}

/**
 * Build a relative deep link, e.g. buildMapsLink({ lat: 59.33, lng: 18.07, label: 'Camp' })
 * → "/maps?lat=59.33&lng=18.07&label=Camp". Other pages can import this to link into Maps.
 */
export const buildMapsLink = (params = {}) => {
  const query = buildMapsQuery(params)
  const search = new URLSearchParams(query).toString()
  return search ? `${MAPS_PATH}?${search}` : MAPS_PATH
}

/** Stable key used to detect whether a route change carries a new deep link. */
export const deepLinkKey = (parsed) => {
  if (!parsed) return ''
  return JSON.stringify([parsed.map, parsed.lat, parsed.lng, parsed.zoom, parsed.label])
}
