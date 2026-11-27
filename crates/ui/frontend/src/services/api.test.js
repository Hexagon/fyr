import { describe, expect, it, vi } from 'vitest'
import { dispatchInferenceEvent, encodePathPreservingSlashes, extractNextSseFrame, mapContentItem, parseSseFrame } from './api.js'

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

describe('inference SSE dispatch', () => {
  const recorder = () => {
    const calls = []
    return {
      calls,
      handlers: {
        onToken: (t) => calls.push(['token', t]),
        onSources: (s) => calls.push(['sources', s]),
        onDone: () => calls.push(['done'])
      }
    }
  }

  it('delivers sources before tokens and stops on done', () => {
    const { calls, handlers } = recorder()
    const stream = [
      'event: sources\ndata: [{"n":1,"title":"Vasa","source":"wiki.zim","link":"/books?file=wiki.zim&path=Vasa"}]',
      'event: token\ndata: Hello [1]',
      'event: done\ndata: ',
      'event: token\ndata: after done'
    ].join('\n\n') + '\n\n'

    let buffer = stream
    let finished = false
    let extracted = extractNextSseFrame(buffer)
    while (extracted && !finished) {
      buffer = extracted.remainder
      finished = dispatchInferenceEvent(parseSseFrame(extracted.frame), handlers)
      extracted = extractNextSseFrame(buffer)
    }

    expect(finished).toBe(true)
    expect(calls).toEqual([
      ['sources', [{ n: 1, title: 'Vasa', source: 'wiki.zim', link: '/books?file=wiki.zim&path=Vasa' }]],
      ['token', 'Hello [1]'],
      ['done']
    ])
  })

  it('treats malformed sources as an empty list and ignores unknown events', () => {
    const { calls, handlers } = recorder()
    expect(dispatchInferenceEvent(parseSseFrame('event: sources\ndata: {oops'), handlers)).toBe(false)
    expect(dispatchInferenceEvent(parseSseFrame('event: ping\ndata: x'), handlers)).toBe(false)
    expect(calls).toEqual([['sources', []]])
  })

  it('works without optional handlers', () => {
    expect(dispatchInferenceEvent({ event: 'sources', data: '[]' }, {})).toBe(false)
    expect(dispatchInferenceEvent({ event: 'done', data: '' })).toBe(true)
  })

  it('keeps leading spaces of token data intact', () => {
    // axum writes `data: ` + value, so a token " world" arrives as `data:  world`.
    const onToken = vi.fn()
    dispatchInferenceEvent(parseSseFrame('event: token\ndata:  world'), { onToken })
    expect(onToken).toHaveBeenCalledWith(' world')
  })
})

describe('streamInference transport', () => {
  it('POSTs the request as JSON so long histories never hit URL limits', async () => {
    const { apiService } = await import('./api.js')
    const encoder = new TextEncoder()
    const calls = []
    const originalFetch = globalThis.fetch
    globalThis.fetch = async (url, init) => {
      calls.push({ url, init })
      const stream = new ReadableStream({
        start(controller) {
          controller.enqueue(encoder.encode('event: token\ndata: Hi\n\nevent: done\ndata:\n\n'))
          controller.close()
        }
      })
      return new Response(stream, { status: 200, headers: { 'Content-Type': 'text/event-stream' } })
    }

    try {
      const history = Array.from({ length: 50 }, (_, i) => ({ role: i % 2 ? 'assistant' : 'user', text: 'x'.repeat(2000) }))
      const tokens = []
      await new Promise((resolve, reject) => {
        apiService.streamInference('model a.gguf', { prompt: 'q?', history, useLibrary: true, sourcesLimit: 3 }, {
          onToken: (token) => tokens.push(token),
          onDone: resolve,
          onError: reject
        })
      })

      expect(calls).toHaveLength(1)
      expect(calls[0].url).toBe('/api/models/model%20a.gguf/infer/stream')
      expect(calls[0].init.method).toBe('POST')
      const body = JSON.parse(calls[0].init.body)
      expect(body).toMatchObject({ prompt: 'q?', use_library: true, sources_limit: 3 })
      expect(body.history).toHaveLength(50)
      expect(tokens).toEqual(['Hi'])
    } finally {
      globalThis.fetch = originalFetch
    }
  })
})
