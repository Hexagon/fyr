import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  MAX_QUERY_LENGTH,
  buildSearchParams,
  countGroupedItems,
  debounce,
  describeSearchError,
  groupResults,
  highlightSegments,
  isSafeLink,
  itemAtIndex,
  moveSelection,
  normalizeQuery,
  normalizeResults
} from './searchResults.js'

const sample = [
  { kind: 'article', title: 'Oslo', score: 980, link: '/books?file=wiki.zim&path=Oslo', source: 'wiki.zim' },
  { kind: 'map', title: 'oslo region', score: 1040, link: '/maps?map=oslo.pmtiles', source: 'oslo.pmtiles' },
  { kind: 'article', title: 'Oslo Airport', score: 790, link: '/books?file=wiki.zim&path=Oslo_Airport', source: 'wiki.zim' },
  { kind: 'poi', title: 'Oslo Hut', score: 800, link: '/maps?lat=59.9&lng=10.7&zoom=14&label=Oslo%20Hut', source: 'huts.geojson', lat: 59.9, lng: 10.7 }
]

describe('normalizeQuery', () => {
  it('trims and collapses whitespace', () => {
    expect(normalizeQuery('  oslo \n  hut ')).toBe('oslo hut')
    expect(normalizeQuery(null)).toBe('')
    expect(normalizeQuery(undefined)).toBe('')
  })

  it('caps length on code points, not UTF-16 units', () => {
    const long = '😀'.repeat(MAX_QUERY_LENGTH + 50)
    const normalized = normalizeQuery(long)
    expect(Array.from(normalized)).toHaveLength(MAX_QUERY_LENGTH)
    expect(normalized.endsWith('\uD83D')).toBe(false)
  })
})

describe('buildSearchParams', () => {
  it('encodes query, limit and kinds', () => {
    expect(buildSearchParams(' café & co ', { limit: 30, kinds: ['book', 'map'] })).toBe(
      'q=caf%C3%A9+%26+co&limit=30&kinds=book%2Cmap'
    )
  })

  it('omits invalid limits and empty kinds', () => {
    expect(buildSearchParams('x', { limit: 'abc', kinds: '' })).toBe('q=x')
    expect(buildSearchParams('x', { limit: -3 })).toBe('q=x')
    expect(buildSearchParams('x', { kinds: 'poi' })).toBe('q=x&kinds=poi')
  })
})

describe('isSafeLink / normalizeResults', () => {
  it('accepts only in-app absolute routes', () => {
    expect(isSafeLink('/books?file=a.epub')).toBe(true)
    expect(isSafeLink('//evil.example/x')).toBe(false)
    expect(isSafeLink('https://evil.example')).toBe(false)
    expect(isSafeLink('javascript:alert(1)')).toBe(false)
    expect(isSafeLink('/\\evil')).toBe(false)
    expect(isSafeLink(null)).toBe(false)
  })

  it('drops malformed items and fills defaults', () => {
    const results = normalizeResults({
      results: [
        null,
        'nope',
        { kind: 'book', title: 'A', link: 'https://x' },
        { kind: 'mystery', link: '/books?file=b.md', source: 'b.md', score: 'NaN' },
        sample[3]
      ]
    })
    expect(results).toHaveLength(2)
    expect(results[0]).toMatchObject({ kind: 'other', title: 'b.md', score: 0, subtitle: '', lat: null })
    expect(results[1]).toMatchObject({ kind: 'poi', lat: 59.9, lng: 10.7 })
    expect(normalizeResults(undefined)).toEqual([])
  })
})

describe('groupResults', () => {
  it('groups by kind, orders groups by best score and indexes items in display order', () => {
    const groups = groupResults(normalizeResults(sample))
    expect(groups.map((g) => g.kind)).toEqual(['map', 'article', 'poi'])
    expect(groups[1].label).toBe('Articles')
    expect(groups[1].items.map((i) => i.title)).toEqual(['Oslo', 'Oslo Airport'])
    expect(groups.flatMap((g) => g.items.map((i) => i.index))).toEqual([0, 1, 2, 3])
    expect(countGroupedItems(groups)).toBe(4)
    expect(itemAtIndex(groups, 2).title).toBe('Oslo Airport')
    expect(itemAtIndex(groups, 9)).toBeNull()
  })

  it('breaks score ties with the fixed kind order', () => {
    const groups = groupResults([
      { kind: 'poi', title: 'p', score: 500, link: '/p' },
      { kind: 'book', title: 'b', score: 500, link: '/b' }
    ])
    expect(groups.map((g) => g.kind)).toEqual(['book', 'poi'])
  })

  it('handles empty input', () => {
    expect(groupResults([])).toEqual([])
    expect(groupResults(null)).toEqual([])
    expect(countGroupedItems([])).toBe(0)
  })
})

describe('moveSelection', () => {
  it('wraps in both directions and starts from nothing selected', () => {
    expect(moveSelection(-1, 1, 3)).toBe(0)
    expect(moveSelection(-1, -1, 3)).toBe(2)
    expect(moveSelection(0, 1, 3)).toBe(1)
    expect(moveSelection(2, 1, 3)).toBe(0)
    expect(moveSelection(0, -1, 3)).toBe(2)
    expect(moveSelection(7, 1, 3)).toBe(0)
    expect(moveSelection(0, 1, 0)).toBe(-1)
  })
})

describe('highlightSegments', () => {
  it('marks case-insensitive matches of each query word', () => {
    expect(highlightSegments('Oslo Airport Gardermoen', 'oslo garder')).toEqual([
      { text: 'Oslo', match: true },
      { text: ' Airport ', match: false },
      { text: 'Garder', match: true },
      { text: 'moen', match: false }
    ])
  })

  it('handles unicode and characters whose lowercase form is longer', () => {
    expect(highlightSegments('ÅLESUND', 'ålesund')).toEqual([{ text: 'ÅLESUND', match: true }])
    // "İ" lowercases to two code units ("i" + combining dot).
    const segments = highlightSegments('İstanbul İzmir', 'İzmir')
    expect(segments.map((s) => s.text).join('')).toBe('İstanbul İzmir')
    expect(segments.find((s) => s.match).text).toBe('İzmir')
  })

  it('returns the plain text for empty queries and nothing for empty text', () => {
    expect(highlightSegments('Oslo', '   ')).toEqual([{ text: 'Oslo', match: false }])
    expect(highlightSegments('', 'oslo')).toEqual([])
    expect(highlightSegments(null, 'oslo')).toEqual([])
  })

  it('treats regex characters literally', () => {
    expect(highlightSegments('C++ (lang)', 'c++ (')).toEqual([
      { text: 'C++', match: true },
      { text: ' ', match: false },
      { text: '(', match: true },
      { text: 'lang)', match: false }
    ])
  })
})

describe('debounce', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it('calls once with the latest arguments', () => {
    const fn = vi.fn()
    const debounced = debounce(fn, 200)
    debounced('o')
    debounced('os')
    vi.advanceTimersByTime(199)
    expect(fn).not.toHaveBeenCalled()
    debounced('osl')
    vi.advanceTimersByTime(200)
    expect(fn).toHaveBeenCalledTimes(1)
    expect(fn).toHaveBeenCalledWith('osl')
  })

  it('supports cancel and flush', () => {
    const fn = vi.fn()
    const debounced = debounce(fn, 200)
    debounced('a')
    debounced.cancel()
    vi.advanceTimersByTime(500)
    expect(fn).not.toHaveBeenCalled()
    debounced('b')
    debounced.flush()
    expect(fn).toHaveBeenCalledWith('b')
    debounced.flush()
    expect(fn).toHaveBeenCalledTimes(1)
  })
})

describe('describeSearchError', () => {
  it('maps statuses to messages', () => {
    expect(describeSearchError({ response: { status: 503 } })).toMatch(/busy/)
    expect(describeSearchError({ response: { status: 500 } })).toMatch(/server/)
    expect(describeSearchError({ response: { status: 400 } })).toMatch(/rejected/)
    expect(describeSearchError({ code: 'ECONNABORTED' })).toMatch(/timed out/)
    expect(describeSearchError(new Error('x'))).toMatch(/reach/)
  })
})
