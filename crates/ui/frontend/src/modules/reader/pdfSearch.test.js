import { describe, expect, it } from 'vitest'
import {
  buildPageText,
  findMatches,
  foldQuery,
  foldWithMap,
  highlightSegments,
  pageResults,
  searchPdfText,
  snippetFor
} from './pdfSearch'

describe('folding', () => {
  it('ignores case, accents and ligatures and collapses whitespace', () => {
    expect(foldWithMap('  Zürich\n\n  CAFÉ ﬁsh ').folded).toBe('zurich cafe fish ')
    expect(foldQuery('  Malmö  ')).toBe('malmo')
  })

  it('maps folded characters back to the source', () => {
    const text = 'aÉﬁb'
    const { folded, starts, ends } = foldWithMap(text)
    expect(folded).toBe('aefib')
    expect(starts).toEqual([0, 1, 2, 2, 3])
    expect(ends).toEqual([1, 2, 3, 3, 4])
  })
})

describe('findMatches', () => {
  it('returns non-overlapping ranges in the original text', () => {
    const text = 'Water, WATER and wäter.'
    const matches = findMatches(text, 'water')
    expect(matches.map((m) => text.slice(m.start, m.end))).toEqual(['Water', 'WATER', 'wäter'])
    expect(findMatches('aaaa', 'aa')).toHaveLength(2)
  })

  it('matches across line breaks and repeated spaces', () => {
    const text = 'boil the\nwater   first'
    const [match] = findMatches(text, 'the water first')
    expect(text.slice(match.start, match.end)).toBe('the\nwater   first')
  })

  it('returns nothing for empty queries and honours the limit', () => {
    expect(findMatches('abc', '   ')).toEqual([])
    expect(findMatches('a a a a', 'a', 2)).toHaveLength(2)
  })
})

describe('buildPageText', () => {
  it('concatenates text items, skipping marked-content entries, with newlines at EOL', () => {
    const page = buildPageText({
      items: [
        { type: 'beginMarkedContent' },
        { str: 'Hello ', hasEOL: false },
        { str: 'world', hasEOL: true },
        { type: 'endMarkedContent' },
        { str: '', hasEOL: false },
        { str: 'Next', hasEOL: false }
      ]
    })
    expect(page.text).toBe('Hello world\nNext')
    expect(page.strs).toEqual(['Hello ', 'world', '', 'Next'])
    expect(page.starts).toEqual([0, 6, 12, 12])
  })

  it('copes with missing content', () => {
    expect(buildPageText(null)).toEqual({ text: '', strs: [], starts: [] })
  })
})

describe('snippets and results', () => {
  it('builds snippets with ellipses around the match', () => {
    const text = `${'lorem '.repeat(30)}needle${' ipsum'.repeat(30)}`
    const start = text.indexOf('needle')
    const snippet = snippetFor(text, start, start + 6, 20)
    expect(snippet.startsWith('… ')).toBe(true)
    expect(snippet.endsWith(' …')).toBe(true)
    expect(snippet).toContain('needle')
    expect(snippetFor('short needle', 6, 12)).toBe('short needle')
  })

  it('numbers occurrences per page', () => {
    const results = pageResults(3, 'fire. Fire!', 'fire')
    expect(results.map((r) => [r.page, r.occurrence, r.title])).toEqual([
      [3, 0, 'Page 3'],
      [3, 1, 'Page 3']
    ])
    expect(new Set(results.map((r) => r.path)).size).toBe(2)
  })
})

describe('searchPdfText', () => {
  const texts = { 1: 'no match here', 2: 'knot and knot', 3: 'final knot' }
  const getPageText = async (n) => buildPageText({ items: [{ str: texts[n], hasEOL: false }] })

  it('searches every page in order', async () => {
    const progress = []
    const outcome = await searchPdfText({
      pageCount: 3,
      getPageText,
      query: 'KNOT',
      onProgress: (page, total) => progress.push(`${page}/${total}`)
    })
    expect(outcome.truncated).toBe(false)
    expect(outcome.results.map((r) => `${r.page}:${r.occurrence}`)).toEqual(['2:0', '2:1', '3:0'])
    expect(progress).toEqual(['1/3', '2/3', '3/3'])
  })

  it('stops at maxResults and reports truncation', async () => {
    const outcome = await searchPdfText({ pageCount: 3, getPageText, query: 'knot', maxResults: 2 })
    expect(outcome.results).toHaveLength(2)
    expect(outcome.truncated).toBe(true)
  })

  it('resolves to null when cancelled (e.g. the user switched books)', async () => {
    let calls = 0
    const outcome = await searchPdfText({
      pageCount: 3,
      getPageText,
      query: 'knot',
      isCancelled: () => ++calls > 1
    })
    expect(outcome).toBeNull()
  })

  it('skips pages whose text cannot be read', async () => {
    const outcome = await searchPdfText({
      pageCount: 3,
      getPageText: (n) => (n === 2 ? Promise.reject(new Error('bad page')) : getPageText(n)),
      query: 'knot'
    })
    expect(outcome.results.map((r) => r.page)).toEqual([3])
  })
})

describe('highlightSegments', () => {
  it('splits matches that span several text items', () => {
    const page = buildPageText({ items: [{ str: 'Purify wa' }, { str: 'ter fast', hasEOL: true }, { str: 'water' }] })
    const matches = findMatches(page.text, 'water')
    expect(matches).toHaveLength(2)
    const segments = highlightSegments(page.strs, page.starts, matches, 1)
    expect(segments.get(0)).toEqual([
      { text: 'Purify ', match: -1 },
      { text: 'wa', match: 0, selected: false }
    ])
    expect(segments.get(1)).toEqual([
      { text: 'ter', match: 0, selected: false },
      { text: ' fast', match: -1 }
    ])
    expect(segments.get(2)).toEqual([{ text: 'water', match: 1, selected: true }])
  })

  it('handles several matches inside one item and empty input', () => {
    const page = buildPageText({ items: [{ str: 'ab ab' }] })
    const segments = highlightSegments(page.strs, page.starts, findMatches(page.text, 'ab'))
    expect(segments.get(0).map((p) => p.text)).toEqual(['ab', ' ', 'ab'])
    expect(highlightSegments(page.strs, page.starts, []).size).toBe(0)
  })
})
