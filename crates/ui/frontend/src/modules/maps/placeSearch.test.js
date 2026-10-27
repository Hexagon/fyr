import { describe, expect, it, vi } from 'vitest'
import { setLocale } from '../../i18n/index.js'
import {
  buildPlaceSearchParams,
  createPlaceSearch,
  isSearchableQuery,
  mapsLinkQuery,
  toPlaceResults
} from './placeSearch.js'
import { humanizeKind, placeKindIcon, placeKindLabel } from './placeKind.js'

const response = {
  results: [
    {
      kind: 'place',
      title: 'Malmö',
      subtitle: 'City',
      place_kind: 'city',
      source: 'sweden.pmtiles',
      score: 1200,
      link: '/maps?map=sweden.pmtiles&lat=55.605&lng=13.0038&zoom=11&label=Malm%C3%B6',
      lat: 55.605,
      lng: 13.0038
    },
    {
      kind: 'poi',
      title: 'Malmö hut',
      source: 'huts.geojson',
      score: 900,
      link: '/maps?lat=55.6&lng=13&zoom=14&label=Malm%C3%B6%20hut',
      lat: 55.6,
      lng: 13
    },
    { kind: 'book', title: 'Malmö guide', link: '/books?file=malmo.md', source: 'malmo.md' },
    { kind: 'place', title: 'Broken', link: '/maps?map=x.pmtiles&lat=999&lng=0', place_kind: 'city' },
    { kind: 'place', title: 'Evil', link: 'javascript:alert(1)', place_kind: 'city' }
  ]
}

describe('place search helpers', () => {
  it('builds the API query for places and POIs', () => {
    expect(buildPlaceSearchParams('  Malmö ')).toBe('q=Malm%C3%B6&limit=8&kinds=place%2Cpoi')
  })

  it('requires two characters', () => {
    expect(isSearchableQuery('å')).toBe(false)
    expect(isSearchableQuery(' å ')).toBe(false)
    expect(isSearchableQuery('Åre')).toBe(true)
  })

  it('parses only Maps deep links with a point', () => {
    expect(mapsLinkQuery('/maps?map=a.pmtiles&lat=1&lng=2&zoom=11&label=X')).toEqual({ map: 'a.pmtiles', lat: '1', lng: '2', zoom: '11', label: 'X' })
    expect(mapsLinkQuery('/maps?map=a.pmtiles')).toBeNull()
    expect(mapsLinkQuery('/books?file=a&lat=1&lng=2')).toBeNull()
    expect(mapsLinkQuery('//evil/maps?lat=1&lng=2')).toBeNull()
    expect(mapsLinkQuery(null)).toBeNull()
  })

  it('keeps place and POI hits with valid links', () => {
    const results = toPlaceResults(response)
    expect(results.map((r) => r.title)).toEqual(['Malmö', 'Malmö hut'])
    expect(results[0]).toMatchObject({
      kind: 'place',
      detail: 'City · on sweden.pmtiles',
      icon: '🏙️',
      query: { map: 'sweden.pmtiles', lat: '55.605', lng: '13.0038', zoom: '11', label: 'Malmö' }
    })
    expect(results[1]).toMatchObject({ kind: 'poi', detail: 'Point of interest · huts.geojson', query: { lat: '55.6', lng: '13', zoom: '14' } })
    expect(toPlaceResults(null)).toEqual([])
  })

  it('translates kinds and falls back for unknown ones', async () => {
    expect(placeKindLabel('national_park')).toBe('National park')
    expect(placeKindLabel('hot_spring')).toBe('Hot spring')
    expect(placeKindLabel('')).toBe('')
    expect(humanizeKind('  under__score-kind ')).toBe('Under score kind')
    expect(placeKindIcon('peak')).toBe('⛰️')
    expect(placeKindIcon('nope')).toBe('📍')
    try {
      await setLocale('sv', { persist: false })
      expect(placeKindLabel('lake')).toBe('Sjö')
      expect(toPlaceResults(response)[0].detail).toBe('Stad · på sweden.pmtiles')
    } finally {
      await setLocale('en', { persist: false })
    }
  })
})

describe('createPlaceSearch', () => {
  const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

  it('debounces, reports states and ignores stale responses', async () => {
    vi.useFakeTimers()
    try {
      const resolvers = []
      const request = vi.fn((q) => new Promise((resolve) => resolvers.push({ q, resolve })))
      const states = []
      const search = createPlaceSearch({ request, onState: (s) => states.push(s) })

      search.search('m')
      expect(states.at(-1)).toMatchObject({ status: 'short', query: 'm' })
      search.search('ma')
      search.search('mal')
      vi.advanceTimersByTime(300)
      expect(request).toHaveBeenCalledTimes(1)
      expect(request.mock.calls[0][0]).toBe('mal')
      expect(states.at(-1)).toMatchObject({ status: 'loading', query: 'mal' })

      // A newer query aborts the older request; its late answer is ignored.
      const firstSignal = request.mock.calls[0][1].signal
      search.search('malmö', { immediate: true })
      expect(firstSignal.aborted).toBe(true)
      resolvers[1].resolve(response)
      await vi.waitFor(() => expect(states.at(-1).status).toBe('done'))
      resolvers[0].resolve({ results: [] })
      await Promise.resolve()
      expect(states.at(-1)).toMatchObject({ status: 'done', query: 'malmö' })
      expect(states.at(-1).results).toHaveLength(2)

      search.search('   ')
      expect(states.at(-1)).toMatchObject({ status: 'idle', results: [] })
    } finally {
      vi.useRealTimers()
    }
  })

  it('reports errors and can be cancelled', async () => {
    const states = []
    const failing = createPlaceSearch({
      request: () => Promise.reject(new Error('down')),
      onState: (s) => states.push(s),
      debounceMs: 0
    })
    failing.search('oslo')
    await flush()
    expect(states.at(-1)).toMatchObject({ status: 'error', query: 'oslo' })
    expect(states.at(-1).error.message).toBe('down')

    const quiet = []
    let resolve
    const slow = createPlaceSearch({
      request: () => new Promise((r) => { resolve = r }),
      onState: (s) => quiet.push(s),
      debounceMs: 0
    })
    slow.search('bergen')
    slow.cancel()
    resolve(response)
    await flush()
    expect(quiet.map((s) => s.status)).toEqual(['loading'])
  })
})
