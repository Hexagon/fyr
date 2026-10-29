// Pure helpers for the global search page (/search) and header search box.
//
// Everything here is framework-free so it can be unit tested with Vitest:
// query normalisation, request params, result validation, grouping by kind,
// keyboard selection and match highlighting.

import { t } from '../../i18n/index.js'
import { placeKindLabel } from '../maps/placeKind.js'

export const MAX_QUERY_LENGTH = 200
export const SEARCH_DEBOUNCE_MS = 250

// Display order of groups when scores tie, and their label message keys.
export const KIND_ORDER = ['book', 'article', 'article_text', 'content', 'map', 'place', 'poi']

export const KIND_LABEL_KEYS = {
  book: 'search.kind.book',
  article: 'search.kind.article',
  article_text: 'search.kind.articleText',
  content: 'search.kind.content',
  map: 'search.kind.map',
  place: 'search.kind.place',
  poi: 'search.kind.poi'
}

/** Translated group heading for a result kind. */
export const kindLabel = (kind) => t(KIND_LABEL_KEYS[kind] || 'search.kind.other')

export const KIND_ICONS = {
  book: '📚',
  article: '📄',
  article_text: '📝',
  content: '🔎',
  map: '🗺️',
  place: '🧭',
  poi: '📍'
}

// Filter chips on the search page; `kinds` is sent to the API.
export const KIND_FILTERS = [
  { id: 'all', labelKey: 'search.filter.all', kinds: '' },
  { id: 'book', labelKey: 'search.filter.book', kinds: 'book' },
  { id: 'article', labelKey: 'search.filter.article', kinds: 'article' },
  { id: 'article_text', labelKey: 'search.filter.articleText', kinds: 'article_text' },
  { id: 'content', labelKey: 'search.filter.content', kinds: 'content' },
  { id: 'map', labelKey: 'search.filter.map', kinds: 'map' },
  { id: 'place', labelKey: 'search.filter.place', kinds: 'place' },
  { id: 'poi', labelKey: 'search.filter.poi', kinds: 'poi' }
]

/** Trim, collapse whitespace and cap the length of a raw query. */
export function normalizeQuery(raw) {
  return Array.from(String(raw ?? '').replace(/\s+/g, ' ').trim())
    .slice(0, MAX_QUERY_LENGTH)
    .join('')
    .trim()
}

/** Build the `/api/search` query string. */
export function buildSearchParams(q, { limit, kinds } = {}) {
  const params = new URLSearchParams()
  params.set('q', normalizeQuery(q))
  const parsedLimit = Number.parseInt(limit, 10)
  if (Number.isFinite(parsedLimit) && parsedLimit > 0) {
    params.set('limit', String(parsedLimit))
  }
  const kindList = Array.isArray(kinds) ? kinds.join(',') : String(kinds || '')
  if (kindList.trim()) {
    params.set('kinds', kindList.trim())
  }
  return params.toString()
}

/**
 * Only same-app absolute routes are navigable ("/books?..."); anything else
 * (protocol-relative "//host", "javascript:", full URLs) is rejected.
 */
export function isSafeLink(link) {
  return typeof link === 'string' && link.startsWith('/') && !link.startsWith('//') && !link.includes('\\')
}

const PLACE_KIND_PATTERN = /^[a-z0-9_]{1,40}$/

/**
 * Subtitle of a `place` hit in the active language: the server sends
 * "<English kind> · <matched alternative name>"; the kind part is replaced
 * by the translated label of `place_kind`.
 */
export function placeSubtitle(subtitle, placeKind) {
  const parts = String(subtitle || '').split(' · ')
  const label = placeKindLabel(placeKind)
  if (!label) return String(subtitle || '')
  parts[0] = label
  return parts.filter(Boolean).join(' · ')
}

/** Drop malformed results and coerce fields into a predictable shape. */
export function normalizeResults(raw) {
  const list = Array.isArray(raw) ? raw : Array.isArray(raw?.results) ? raw.results : []
  return list
    .filter((item) => item && typeof item === 'object' && isSafeLink(item.link))
    .map((item) => ({
      kind: KIND_ORDER.includes(item.kind) ? item.kind : 'other',
      title: String(item.title || item.source || t('search.untitled')),
      placeKind: typeof item.place_kind === 'string' && PLACE_KIND_PATTERN.test(item.place_kind) ? item.place_kind : '',
      subtitle: item.kind === 'place' && typeof item.place_kind === 'string' && PLACE_KIND_PATTERN.test(item.place_kind)
        ? placeSubtitle(item.subtitle, item.place_kind)
        : item.subtitle ? String(item.subtitle) : '',
      source: String(item.source || ''),
      score: Number.isFinite(Number(item.score)) ? Number(item.score) : 0,
      link: item.link,
      snippet: normalizeSnippet(item.snippet),
      lat: Number.isFinite(item.lat) ? item.lat : null,
      lng: Number.isFinite(item.lng) ? item.lng : null
    }))
}

const MAX_SNIPPET_SEGMENTS = 64
const MAX_SNIPPET_CHARS = 600

/**
 * Validate a server snippet (`[{ text, match }]`, used by `article_text`
 * hits) into plain-text segments. Text is only ever rendered with text
 * interpolation, never as HTML, so markup inside articles stays inert.
 * Returns null when there is nothing usable.
 */
export function normalizeSnippet(raw) {
  if (!Array.isArray(raw)) return null
  const segments = []
  let used = 0
  for (const item of raw.slice(0, MAX_SNIPPET_SEGMENTS)) {
    if (!item || typeof item.text !== 'string' || !item.text) continue
    // eslint-disable-next-line no-control-regex
    const text = item.text.replace(/[\u0000-\u001f\u007f]/g, ' ').slice(0, MAX_SNIPPET_CHARS - used)
    if (!text) break
    used += text.length
    const match = item.match === true
    const last = segments[segments.length - 1]
    if (last && last.match === match) last.text += text
    else segments.push({ text, match })
    if (used >= MAX_SNIPPET_CHARS) break
  }
  return segments.length ? segments : null
}

/**
 * Group results by kind. Groups are ordered by their best score (most
 * relevant group first, ties by KIND_ORDER); items keep the server's order
 * within a group. Each item gets a flat `index` matching display order, used
 * for keyboard navigation.
 */
export function groupResults(results) {
  const groups = new Map()
  for (const result of results || []) {
    const kind = result.kind || 'other'
    if (!groups.has(kind)) {
      groups.set(kind, {
        kind,
        label: kindLabel(kind),
        icon: KIND_ICONS[kind] || '•',
        best: -Infinity,
        items: []
      })
    }
    const group = groups.get(kind)
    group.items.push(result)
    group.best = Math.max(group.best, Number(result.score) || 0)
  }

  const rank = (kind) => {
    const i = KIND_ORDER.indexOf(kind)
    return i === -1 ? KIND_ORDER.length : i
  }
  const ordered = [...groups.values()].sort((a, b) => b.best - a.best || rank(a.kind) - rank(b.kind))

  let index = 0
  return ordered.map((group) => ({
    kind: group.kind,
    label: group.label,
    icon: group.icon,
    items: group.items.map((item) => ({ ...item, index: index++ }))
  }))
}

/** Total number of items across groups. */
export function countGroupedItems(groups) {
  return (groups || []).reduce((sum, group) => sum + group.items.length, 0)
}

/** Item at a flat display index, or null. */
export function itemAtIndex(groups, index) {
  for (const group of groups || []) {
    for (const item of group.items) {
      if (item.index === index) return item
    }
  }
  return null
}

/**
 * Move the keyboard selection by `delta`, wrapping around. `-1` means
 * "nothing selected"; moving down from it selects the first item and moving
 * up selects the last.
 */
export function moveSelection(current, delta, count) {
  if (!count || count <= 0) return -1
  if (current < 0 || current >= count) {
    return delta >= 0 ? 0 : count - 1
  }
  return (((current + delta) % count) + count) % count
}

/**
 * Split `text` into segments marking case-insensitive matches of the query's
 * words, for safe rendering without v-html.
 * Returns [{ text, match }]; adjacent segments never share the same flag.
 */
export function highlightSegments(text, query) {
  const source = String(text ?? '')
  const terms = [...new Set(normalizeQuery(query).toLocaleLowerCase().split(' ').filter(Boolean))]
  if (!source || !terms.length) {
    return source ? [{ text: source, match: false }] : []
  }

  // Lowercase per code point so offsets map back to the original string even
  // when lowercasing changes a character's length (e.g. "İ").
  const chars = Array.from(source)
  let lower = ''
  const lowerToChar = []
  chars.forEach((ch, i) => {
    const lc = ch.toLocaleLowerCase()
    lower += lc
    for (let k = 0; k < lc.length; k += 1) lowerToChar.push(i)
  })

  const marked = new Array(chars.length).fill(false)
  for (const term of terms) {
    let from = 0
    while (from <= lower.length - term.length) {
      const at = lower.indexOf(term, from)
      if (at === -1) break
      const start = lowerToChar[at]
      const endExclusive = at + term.length < lowerToChar.length ? lowerToChar[at + term.length] : chars.length
      for (let i = start; i < Math.max(endExclusive, start + 1); i += 1) marked[i] = true
      from = at + term.length
    }
  }

  const segments = []
  chars.forEach((ch, i) => {
    const last = segments[segments.length - 1]
    if (last && last.match === marked[i]) {
      last.text += ch
    } else {
      segments.push({ text: ch, match: marked[i] })
    }
  })
  return segments
}

/** Debounce `fn`; the returned function has `.cancel()` and `.flush()`. */
export function debounce(fn, waitMs = SEARCH_DEBOUNCE_MS) {
  let timer = null
  let pendingArgs = null
  const debounced = (...args) => {
    pendingArgs = args
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => {
      timer = null
      const callArgs = pendingArgs
      pendingArgs = null
      fn(...callArgs)
    }, waitMs)
  }
  debounced.cancel = () => {
    if (timer) clearTimeout(timer)
    timer = null
    pendingArgs = null
  }
  debounced.flush = () => {
    if (!timer) return
    clearTimeout(timer)
    timer = null
    const callArgs = pendingArgs
    pendingArgs = null
    fn(...callArgs)
  }
  return debounced
}

/** Human-readable error for a failed search request. */
export function describeSearchError(error) {
  const status = error?.response?.status
  if (status === 503) return t('search.errors.busy')
  if (status >= 500) return t('search.errors.server')
  if (status >= 400) return t('search.errors.rejected')
  if (error?.code === 'ECONNABORTED') return t('search.errors.timeout')
  return t('search.errors.unreachable')
}
