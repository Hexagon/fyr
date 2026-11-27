import { describe, expect, it } from 'vitest'
import { citedNumbers, linkCitations, normalizeSources, parseSourcesEvent } from './citations.js'

const sources = [
  { n: 1, title: 'Gustav Vasa', source: 'wikipedia_sv.zim', link: '/books?file=wikipedia_sv.zim&path=Gustav_Vasa' },
  { n: 2, title: 'Survival › "Water"', source: 'survival.md', link: '/books?file=survival.md&q=boil%20water' }
]

describe('normalizeSources', () => {
  it('keeps valid entries sorted and drops malformed ones', () => {
    const result = normalizeSources([
      { n: 2, title: 'B', source: 'b.md', link: '/books?file=b.md' },
      { n: 1, title: '', source: 'a.zim', link: '/books?file=a.zim' },
      { n: 1, title: 'duplicate', source: 'x', link: '/x' },
      { n: 0, title: 'zero' },
      { n: 'x', title: 'nan' },
      null,
      'text'
    ])
    expect(result).toEqual([
      { n: 1, title: 'a.zim', source: 'a.zim', link: '/books?file=a.zim' },
      { n: 2, title: 'B', source: 'b.md', link: '/books?file=b.md' }
    ])
  })

  it('rejects links that could leave the app', () => {
    const result = normalizeSources([
      { n: 1, title: 'a', link: 'javascript:alert(1)' },
      { n: 2, title: 'b', link: '//evil.example/x' },
      { n: 3, title: 'c', link: 'https://evil.example' }
    ])
    expect(result.map((s) => s.link)).toEqual(['', '', ''])
  })

  it('returns [] for non-arrays', () => {
    expect(normalizeSources({ n: 1 })).toEqual([])
    expect(normalizeSources(undefined)).toEqual([])
  })
})

describe('parseSourcesEvent', () => {
  it('parses the SSE sources payload', () => {
    expect(parseSourcesEvent(JSON.stringify(sources))).toEqual(sources)
    expect(parseSourcesEvent('[]')).toEqual([])
  })

  it('tolerates malformed payloads', () => {
    expect(parseSourcesEvent('not json')).toEqual([])
    expect(parseSourcesEvent('')).toEqual([])
    expect(parseSourcesEvent('{"n":1}')).toEqual([])
  })
})

describe('linkCitations', () => {
  it('links known citations and escapes attributes', () => {
    const out = linkCitations('Vasa was king [1]. Boil water [2].', sources)
    expect(out).toContain('<a class="citation" href="/books?file=wikipedia_sv.zim&amp;path=Gustav_Vasa" data-citation="1" title="Gustav Vasa — wikipedia_sv.zim">&#91;1&#93;</a>')
    expect(out).toContain('title="Survival › &quot;Water&quot; — survival.md"')
    expect(out).not.toContain('[1]')
  })

  it('splits grouped citations and leaves unknown numbers as text', () => {
    const out = linkCitations('See [1, 2] and [1;7] but not [9].', sources)
    expect((out.match(/data-citation="1"/g) || []).length).toBe(2)
    expect(out).toContain('data-citation="2"')
    expect(out).toContain('[7]')
    expect(out).toContain('[9]')
  })

  it('ignores markdown links, reference definitions and code', () => {
    const md = [
      'A [1](http://example.com) link.',
      '[1]: http://example.com',
      'Inline `arr[1]` code and real [2].',
      '```',
      'x = list[1]',
      '```',
      'After fence [1].'
    ].join('\n')
    const out = linkCitations(md, sources)
    const lines = out.split('\n')
    expect(lines[0]).toBe('A [1](http://example.com) link.')
    expect(lines[1]).toBe('[1]: http://example.com')
    expect(lines[2]).toContain('`arr[1]`')
    expect(lines[2]).toContain('data-citation="2"')
    expect(lines[4]).toBe('x = list[1]')
    expect(lines[6]).toContain('data-citation="1"')
  })

  it('returns text unchanged without sources', () => {
    expect(linkCitations('Answer [1].', [])).toBe('Answer [1].')
    expect(linkCitations(null, sources)).toBe('')
  })

  it('renders sources without a safe link as plain markers', () => {
    const out = linkCitations('Fact [1].', [{ n: 1, title: 'T', source: 's', link: 'javascript:x' }])
    expect(out).toContain('<span class="citation" data-citation="1"')
    expect(out).not.toContain('href')
  })

  it('handles partial streamed citations', () => {
    expect(linkCitations('Streaming [1', sources)).toBe('Streaming [1')
  })
})

describe('citedNumbers', () => {
  it('collects cited source numbers outside code', () => {
    expect([...citedNumbers('Uses [2] and `[1]`.', sources)]).toEqual([2])
  })
})
