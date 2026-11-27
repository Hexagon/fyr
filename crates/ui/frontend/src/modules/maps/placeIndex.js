// Pure helpers for the per-map place index status shown in the Content
// Manager (GET /api/places/status, POST /api/places/:f/build,
// DELETE /api/places/:f). Framework-free so they can be unit tested.

import { formatNumber, t } from '../../i18n/index.js'

export const PLACES_POLL_MS = 3000

const STATES = new Set([
  'not_indexed',
  'queued',
  'indexing',
  'paused',
  'ready',
  'failed',
  'excluded',
  'disabled',
  'unsupported'
])

const CONTENT = new Set(['vector', 'raster', 'unknown'])

const finite = (value, fallback = 0) => (Number.isFinite(Number(value)) ? Number(value) : fallback)

/** True for map filenames the place indexer handles. */
export function isMapFilename(name) {
  return typeof name === 'string' && /\.(pmtiles|mbtiles)$/i.test(name)
}

/**
 * Normalize the status payload into `{ enabled, lowDisk, byFile }` where
 * `byFile` maps filename → entry. Malformed entries are dropped; unknown
 * states become `not_indexed`.
 */
export function normalizePlaceStatus(raw) {
  const byFile = new Map()
  const maps = Array.isArray(raw?.maps) ? raw.maps : []
  for (const item of maps) {
    if (!item || typeof item.filename !== 'string' || !item.filename) continue
    byFile.set(item.filename, {
      filename: item.filename,
      state: STATES.has(item.state) ? item.state : 'not_indexed',
      content: CONTENT.has(item.content) ? item.content : 'unknown',
      percent: Math.min(100, Math.max(0, finite(item.percent))),
      places: Math.max(0, finite(item.places)),
      auto: item.auto === true,
      indexBytes: Math.max(0, finite(item.index_bytes)),
      note: typeof item.note === 'string' ? item.note : ''
    })
  }
  return {
    enabled: raw?.enabled !== false,
    lowDisk: raw?.low_disk === true,
    byFile
  }
}

const count = (n) => formatNumber(Number(n || 0))

/**
 * Display info for one map: `{ label, tone, showProgress, percent, canBuild,
 * canDelete, title }`. The page additionally hides the actions when
 * isAdminLocked().
 */
export function describePlaceEntry(entry, { enabled = true } = {}) {
  const base = { showProgress: false, percent: 0, canBuild: false, canDelete: false, title: '' }
  if (entry?.state === 'unsupported') {
    return entry.content === 'raster'
      ? { ...base, label: t('content.places.raster'), tone: 'muted', title: t('content.places.rasterHint') }
      : { ...base, label: t('content.places.unsupported'), tone: 'muted', title: entry.note || '' }
  }
  if (!enabled) {
    return { ...base, label: t('content.places.off'), tone: 'muted', title: t('content.places.offHint') }
  }
  if (!entry) {
    return { ...base, label: t('content.places.notIndexed'), tone: 'muted', canBuild: true }
  }
  const pct = Math.round(entry.percent)
  const known = { ...base, percent: entry.percent, title: entry.note || '' }
  const places = Number(entry.places) || 0
  switch (entry.state) {
    case 'ready':
      return {
        ...known,
        label: t('content.places.ready'),
        tone: 'ok',
        canDelete: true,
        title: [t('content.places.placesIndexed', { count: count(places) }, places), entry.note].filter(Boolean).join(' · ')
      }
    case 'indexing':
      return { ...known, label: t('content.places.indexing', { pct }), tone: 'busy', showProgress: true, canDelete: true, title: t('content.places.placesSoFar', { count: count(places) }, places) }
    case 'queued':
      return { ...known, label: t('content.places.queued'), tone: 'busy', canDelete: true }
    case 'paused':
      return { ...known, label: t('content.places.paused', { pct }), tone: 'warn', showProgress: true, canBuild: true, canDelete: true, title: entry.note || t('content.places.resumes') }
    case 'failed':
      return { ...known, label: t('content.places.failed'), tone: 'error', canBuild: true, canDelete: true }
    case 'excluded':
      return { ...known, label: t('content.places.notIndexed'), tone: 'muted', canBuild: true, title: t('content.places.excludedHint') }
    case 'disabled':
      return { ...known, label: t('content.places.off'), tone: 'muted' }
    default:
      return {
        ...known,
        label: entry.auto ? t('content.places.waiting') : t('content.places.notIndexed'),
        tone: 'muted',
        canBuild: true,
        title: entry.auto ? t('content.places.autoHint') : t('content.places.manualHint')
      }
  }
}

/** Keep polling while any map is being indexed or about to be. */
export function shouldPollPlaces(status) {
  if (!status?.byFile) return false
  for (const entry of status.byFile.values()) {
    if (entry.state === 'indexing' || entry.state === 'queued') return true
    if (entry.state === 'not_indexed' && entry.auto && entry.content === 'vector') return true
  }
  return false
}
