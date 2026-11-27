// Pure helpers for the per-archive full-text index status shown in the
// Content Manager (GET /api/index/status, POST /api/index/:f/build,
// DELETE /api/index/:f). Framework-free so they can be unit tested.

import { formatNumber, t } from '../../i18n/index.js'

export const INDEX_POLL_MS = 3000

const STATES = new Set([
  'not_indexed',
  'queued',
  'indexing',
  'paused',
  'ready',
  'failed',
  'excluded',
  'disabled'
])

const finite = (value, fallback = 0) => (Number.isFinite(Number(value)) ? Number(value) : fallback)

/** True for filenames the indexer handles (`.zim`, any case). */
export function isZimFilename(name) {
  return typeof name === 'string' && /\.zim$/i.test(name)
}

/**
 * Normalize the status payload into `{ enabled, lowDisk, autoMaxArticles,
 * byFile }` where `byFile` maps filename → entry. Malformed entries are
 * dropped; unknown states become `not_indexed`.
 */
export function normalizeIndexStatus(raw) {
  const byFile = new Map()
  const archives = Array.isArray(raw?.archives) ? raw.archives : []
  for (const item of archives) {
    if (!item || typeof item.filename !== 'string' || !item.filename) continue
    byFile.set(item.filename, {
      filename: item.filename,
      state: STATES.has(item.state) ? item.state : 'not_indexed',
      percent: Math.min(100, Math.max(0, finite(item.percent))),
      articles: Number.isFinite(item.articles) ? item.articles : null,
      indexedArticles: Math.max(0, finite(item.indexed_articles)),
      auto: item.auto === true,
      indexBytes: Math.max(0, finite(item.index_bytes)),
      note: typeof item.note === 'string' ? item.note : ''
    })
  }
  return {
    enabled: raw?.enabled !== false,
    lowDisk: raw?.low_disk === true,
    autoMaxArticles: finite(raw?.auto_max_articles),
    byFile
  }
}

const formatCount = (n) => formatNumber(Number(n || 0))

/**
 * Display info for one archive: `{ label, tone, showProgress, percent,
 * canBuild, canDelete, title }`. `canBuild` / `canDelete` say which admin
 * actions make sense; the page additionally hides them when isAdminLocked().
 */
export function describeIndexEntry(entry, { enabled = true } = {}) {
  const base = { showProgress: false, percent: 0, canBuild: false, canDelete: false, title: '' }
  if (!enabled) {
    return { ...base, label: t('content.index.off'), tone: 'muted', title: t('content.index.offHint') }
  }
  if (!entry) {
    return { ...base, label: t('content.index.notIndexed'), tone: 'muted', canBuild: true }
  }
  const pct = Math.round(entry.percent)
  const known = { ...base, percent: entry.percent, title: entry.note || '' }
  switch (entry.state) {
    case 'ready':
      return { ...known, label: t('content.index.ready'), tone: 'ok', canDelete: true, title: t('content.index.articlesIndexed', { count: formatCount(entry.indexedArticles) }, Number(entry.indexedArticles) || 0) }
    case 'indexing':
      return { ...known, label: t('content.index.indexing', { pct }), tone: 'busy', showProgress: true, canDelete: true, title: t('content.index.articlesSoFar', { count: formatCount(entry.indexedArticles) }, Number(entry.indexedArticles) || 0) }
    case 'queued':
      return { ...known, label: t('content.index.queued'), tone: 'busy', canDelete: true }
    case 'paused':
      return { ...known, label: t('content.index.paused', { pct }), tone: 'warn', showProgress: true, canBuild: true, canDelete: true, title: entry.note || t('content.index.resumes') }
    case 'failed':
      return { ...known, label: t('content.index.failed'), tone: 'error', canBuild: true, canDelete: true }
    case 'excluded':
      return { ...known, label: t('content.index.notIndexed'), tone: 'muted', canBuild: true, title: t('content.index.excludedHint') }
    case 'disabled':
      return { ...known, label: t('content.index.off'), tone: 'muted' }
    default:
      return {
        ...known,
        label: entry.auto ? t('content.index.waiting') : t('content.index.notIndexed'),
        tone: 'muted',
        canBuild: true,
        title: entry.auto
          ? t('content.index.autoHint')
          : t('content.index.manualHint')
      }
  }
}

/** Keep polling while any archive is being indexed or about to be. */
export function shouldPollIndex(status) {
  if (!status?.byFile) return false
  for (const entry of status.byFile.values()) {
    if (entry.state === 'indexing' || entry.state === 'queued') return true
    if (entry.state === 'not_indexed' && entry.auto) return true
  }
  return false
}
