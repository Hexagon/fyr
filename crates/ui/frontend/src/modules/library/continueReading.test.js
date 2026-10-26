import { describe, expect, it } from 'vitest'
import { formatRelativeTime, selectContinueReading } from './continueReading'

describe('selectContinueReading', () => {
  const recent = [
    { filename: 'gone.epub', lastOpened: 30, bookmarks: [] },
    { filename: 'wiki.zim', lastOpened: 20, title: 'Old title', format: 'zim', position: { type: 'zim', path: 'A/X', label: 'Earth' }, bookmarks: [{}, {}] },
    { filename: 'notes & more.md', lastOpened: 10, bookmarks: [] }
  ]
  const books = [{ filename: 'wiki.zim', title: 'Wikipedia' }, { filename: 'notes & more.md' }, { filename: 'unread.pdf' }]

  it('keeps only books still in the library, in recency order', () => {
    const items = selectContinueReading(recent, books)
    expect(items.map((i) => i.filename)).toEqual(['wiki.zim', 'notes & more.md'])
    expect(items[0]).toMatchObject({
      title: 'Wikipedia',
      format: 'ZIM',
      positionLabel: 'Earth',
      bookmarkCount: 2,
      href: '/books?file=wiki.zim'
    })
    expect(items[1]).toMatchObject({ title: 'notes & more', format: 'MD', href: '/books?file=notes+%26+more.md' })
  })

  it('honours the limit and tolerates bad input', () => {
    expect(selectContinueReading(recent, books, 1)).toHaveLength(1)
    expect(selectContinueReading(null, books)).toEqual([])
    expect(selectContinueReading(recent, undefined)).toEqual([])
    expect(selectContinueReading([null, { filename: 'wiki.zim' }], [null, { filename: 'wiki.zim' }])).toHaveLength(1)
  })
})

describe('formatRelativeTime', () => {
  const now = 10 * 24 * 60 * 60 * 1000
  it('formats recent times', () => {
    expect(formatRelativeTime(0, now)).toBe('')
    expect(formatRelativeTime(now - 5_000, now)).toBe('just now')
    expect(formatRelativeTime(now - 5 * 60_000, now)).toBe('5 min ago')
    expect(formatRelativeTime(now - 60 * 60_000, now)).toBe('1 hour ago')
    expect(formatRelativeTime(now - 3 * 24 * 60 * 60_000, now)).toBe('3 days ago')
  })
})
