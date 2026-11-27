import { describe, expect, it } from 'vitest'
import { createReaderFactory } from './useReaderFactory.js'

describe('createReaderFactory', () => {
  const zimReader = { name: 'zim' }
  const factory = createReaderFactory({ zim: zimReader })

  it('detects formats case-insensitively', () => {
    expect(factory.detectFormat('Wiki.ZIM')).toBe('zim')
    expect(factory.detectFormat('book.epub')).toBe('epub')
    expect(factory.detectFormat('notes.txt')).toBe('unknown')
  })

  it('builds an encoded content URL for a selection', () => {
    expect(factory.inferDescriptorFromSelection({ filename: 'my book.epub' })).toEqual({
      filename: 'my book.epub',
      format: 'epub',
      content_url: '/docs/books/my%20book.epub'
    })
  })

  it('returns registered readers only', () => {
    expect(factory.getReader('zim')).toBe(zimReader)
    expect(factory.getReader('pdf')).toBeNull()
  })
})
