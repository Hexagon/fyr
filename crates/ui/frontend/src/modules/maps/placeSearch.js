// Place search on the Maps page: queries global search for `place` hits
// (names from indexed vector maps) and `poi` hits (POI files), and turns
// them into map targets. Framework-free so it can be unit tested.
//
// Results navigate with the maps deep-link contract (deepLink.js), so the
// page applies them exactly like any other /maps?map=&lat=&lng=&zoom=&label=
// link: switch map if needed, centre, and show a labelled marker.

import { t } from '../../i18n/index.js'
import { buildSearchParams, normalizeQuery, normalizeResults } from '../search/searchResults.js'
import { MAPS_PATH, parseMapsQuery } from './deepLink.js'
import { placeKindIcon, placeKindLabel } from './placeKind.js'

export const PLACE_SEARCH_KINDS = 'place,poi'
export const PLACE_SEARCH_LIMIT = 8
export const MIN_PLACE_QUERY_CHARS = 2
export const PLACE_SEARCH_DEBOUNCE_MS = 250

/** `/api/search` query string for the Maps page search box. */
export function buildPlaceSearchParams(q) {
  return buildSearchParams(q, { limit: PLACE_SEARCH_LIMIT, kinds: PLACE_SEARCH_KINDS })
}

/** Whether a query is long enough to send. */
export function isSearchableQuery(q) {
  return Array.from(normalizeQuery(q)).length >= MIN_PLACE_QUERY_CHARS
}

/**
 * Turn a `/maps?...` link into a router query object, or null for anything
 * that is not a Maps deep link with a valid point.
 */
export function mapsLinkQuery(link) {
  if (typeof link !== 'string' || !link.startsWith(`${MAPS_PATH}?`)) return null
  const params = new URLSearchParams(link.slice(MAPS_PATH.length + 1))
  const query = {}
  for (const [key, value] of params) {
    if (!(key in query)) query[key] = value
  }
  return parseMapsQuery(query).hasPoint ? query : null
}

/**
 * Normalise an `/api/search` response into Maps page results:
 * `{ id, kind, title, detail, icon, query, lat, lng }`. Only place and POI
 * hits with a valid Maps deep link survive.
 */
export function toPlaceResults(raw) {
  const out = []
  const seen = new Set()
  for (const item of normalizeResults(raw)) {
    if (item.kind !== 'place' && item.kind !== 'poi') continue
    const query = mapsLinkQuery(item.link)
    if (!query || seen.has(item.link)) continue
    seen.add(item.link)
    const detail = item.kind === 'place'
      ? [item.subtitle || placeKindLabel(item.placeKind), item.source ? t('maps.search.onMap', { map: item.source }) : '']
      : [placeKindLabel('poi'), item.source]
    out.push({
      id: item.link,
      kind: item.kind,
      title: item.title,
      detail: detail.filter(Boolean).join(' · '),
      icon: item.kind === 'place' ? placeKindIcon(item.placeKind) : '📍',
      query,
      lat: item.lat,
      lng: item.lng
    })
  }
  return out
}

/**
 * Debounced, latest-wins search controller.
 *
 * `request(q, { signal })` performs the HTTP call and resolves to the raw
 * response; `onState(state)` receives `{ status, query, results, error }`
 * where status is `idle`, `short`, `loading`, `done` or `error`. A newer
 * query aborts the request of an older one, and late responses for older
 * queries are ignored.
 */
export function createPlaceSearch({ request, onState, debounceMs = PLACE_SEARCH_DEBOUNCE_MS, setTimer = setTimeout, clearTimer = clearTimeout }) {
  let timer = null
  let seq = 0
  let controller = null

  const emit = (state) => onState({ results: [], error: null, ...state })

  const abort = () => {
    if (controller) controller.abort()
    controller = null
  }

  const run = async (q, id) => {
    abort()
    const ctrl = typeof AbortController === 'function' ? new AbortController() : null
    controller = ctrl
    emit({ status: 'loading', query: q })
    try {
      const raw = await request(q, { signal: ctrl?.signal })
      if (id !== seq) return
      emit({ status: 'done', query: q, results: toPlaceResults(raw) })
    } catch (error) {
      if (id !== seq) return
      emit({ status: 'error', query: q, error })
    } finally {
      if (controller === ctrl) controller = null
    }
  }

  const cancel = () => {
    seq += 1
    if (timer) clearTimer(timer)
    timer = null
    abort()
  }

  /** Schedule a search for `raw` (debounced unless `immediate`). */
  const search = (raw, { immediate = false } = {}) => {
    cancel()
    const q = normalizeQuery(raw)
    if (!q) {
      emit({ status: 'idle', query: '' })
      return
    }
    if (!isSearchableQuery(q)) {
      emit({ status: 'short', query: q })
      return
    }
    const id = seq
    if (immediate || debounceMs <= 0) {
      run(q, id)
    } else {
      timer = setTimer(() => {
        timer = null
        run(q, id)
      }, debounceMs)
    }
  }

  return { search, cancel }
}
