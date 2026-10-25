import { describe, expect, it } from 'vitest'
import { encodePathPreservingSlashes, extractNextSseFrame, mapContentItem, parseSseFrame } from './api.js'

describe('mapContentItem', () => {
  it('maps server field names to UI field names', () => {
    const item = mapContentItem({ name: 'atlas.pmtiles', file_size: 42, file_path: 'atlas.pmtiles', created_at: 'now' })
    expect(item).toMatchObject({ filename: 'atlas.pmtiles', size: 42, path: 'atlas.pmtiles', modified: 'now' })
  })
})

describe('encodePathPreservingSlashes', () => {
  it('encodes each segment but keeps separators', () => {
    expect(encodePathPreservingSlashes('/A/Main Page')).toBe('A/Main%20Page')
  })

  it('does not double-encode already encoded segments', () => {
    expect(encodePathPreservingSlashes('A/Main%20Page')).toBe('A/Main%20Page')
  })
})

describe('SSE parsing', () => {
  it('parses event name and multi-line data', () => {
    expect(parseSseFrame('event: token\ndata: hello\ndata: world')).toEqual({ event: 'token', data: 'hello\nworld' })
  })

  it('defaults to message events and ignores comments', () => {
    expect(parseSseFrame(': keep-alive\ndata: x')).toEqual({ event: 'message', data: 'x' })
  })

  it('splits frames on blank lines and keeps the remainder', () => {
    expect(extractNextSseFrame('data: a\n\ndata: b')).toEqual({ frame: 'data: a', remainder: 'data: b' })
    expect(extractNextSseFrame('data: incomplete')).toBeNull()
  })
})
