import { describe, expect, it } from 'vitest'
import { parseQuery } from 'vue-router'
import { buildBookDeepLink, buildReaderQuery, bookRoute, parseReaderQuery, readerStateEquals } from './deepLink'

describe('parseReaderQuery', () => {
  it('returns empty state without a file', () => {
    expect(parseReaderQuery({})).toEqual({ file: null, path: null, q: null })
    expect(parseReaderQuery(null)).toEqual({ file: null, path: null, q: null })
    expect(parseReaderQuery({ path: 'A/B', q: 'x' })).toEqual({ file: null, path: null, q: null })
  })

  it('parses file, path and q', () => {
    expect(parseReaderQuery({ file: 'wiki.zim', path: '/A/Earth#History', q: '  plate   tectonics ' })).toEqual({
      file: 'wiki.zim',
      path: 'A/Earth#History',
      q: 'plate tectonics'
    })
  })

  it('takes the first string of repeated params', () => {
    expect(parseReaderQuery({ file: [null, 'a.epub', 'b.epub'] }).file).toBe('a.epub')
  })

  it('rejects unsafe or invalid file names', () => {
    for (const file of ['', '../etc/passwd', 'dir/x.epub', 'a\\b.epub', '..', 'bad\u0000.zim', 'x'.repeat(2000)]) {
      expect(parseReaderQuery({ file }).file).toBeNull()
    }
  })

  it('drops empty or invalid path and q', () => {
    expect(parseReaderQuery({ file: 'a.zim', path: '///', q: '   ' })).toEqual({ file: 'a.zim', path: null, q: null })
    expect(parseReaderQuery({ file: 'a.zim', path: 'A/\nB' }).path).toBeNull()
    expect(parseReaderQuery({ file: 'a.zim', q: 'x'.repeat(500) }).q).toHaveLength(256)
  })
})

describe('buildReaderQuery / readerStateEquals', () => {
  it('omits empty fields', () => {
    expect(buildReaderQuery({ file: 'a.epub', path: null, q: '' })).toEqual({ file: 'a.epub' })
    expect(buildReaderQuery({ file: null, path: 'A/B' })).toEqual({})
    expect(buildReaderQuery({ file: 'w.zim', path: 'A/B', q: 'x' })).toEqual({ file: 'w.zim', path: 'A/B', q: 'x' })
  })

  it('compares normalized state', () => {
    expect(readerStateEquals({ file: 'a.zim', path: '/A/B' }, { file: 'a.zim', path: 'A/B', q: '' })).toBe(true)
    expect(readerStateEquals({}, { file: null })).toBe(true)
    expect(readerStateEquals({ file: 'a.zim' }, { file: 'b.zim' })).toBe(false)
    expect(readerStateEquals({ file: 'a.zim', q: 'x' }, { file: 'a.zim' })).toBe(false)
  })

  it('bookRoute returns a router location', () => {
    expect(bookRoute('a.md', { q: 'install' })).toEqual({ path: '/books', query: { file: 'a.md', q: 'install' } })
  })
})

describe('buildBookDeepLink', () => {
  it('builds plain links', () => {
    expect(buildBookDeepLink('a.epub')).toBe('/books?file=a.epub')
    expect(buildBookDeepLink(null)).toBe('/books')
  })

  it('round-trips special characters through vue-router query parsing', () => {
    const file = 'C++ & Rust: 100% #1 (ä).zim'
    const path = 'A/C++_(programming language)?x=1&y=2#Hist%C3%B6ry'
    const q = 'a+b & c=d #e'
    const href = buildBookDeepLink(file, { path, q })
    expect(href.startsWith('/books?')).toBe(true)
    const parsed = parseReaderQuery(parseQuery(href.slice('/books?'.length)))
    expect(parsed).toEqual({ file, path, q })
  })
})
