import { describe, expect, it, vi } from 'vitest'
import { backoffDelay, createChannelStream } from './stream.js'

const encoder = new TextEncoder()

/** Response whose body yields the given chunks, then ends (or hangs). */
const sseResponse = (chunks, { hang = false, status = 200 } = {}) => {
  let i = 0
  return {
    ok: status >= 200 && status < 300,
    status,
    body: {
      getReader: () => ({
        read: () =>
          i < chunks.length
            ? Promise.resolve({ value: encoder.encode(chunks[i++]), done: false })
            : hang
              ? new Promise(() => {})
              : Promise.resolve({ done: true })
      })
    }
  }
}

const flush = () => new Promise((r) => setTimeout(r, 0))

describe('backoffDelay', () => {
  it('grows exponentially with jitter and caps', () => {
    expect(backoffDelay(0, {}, () => 1)).toBe(1000)
    expect(backoffDelay(0, {}, () => 0)).toBe(500)
    expect(backoffDelay(3, {}, () => 1)).toBe(8000)
    expect(backoffDelay(20, {}, () => 1)).toBe(30000)
  })
})

describe('createChannelStream', () => {
  it('dispatches events, catches up on ready and after reconnects', async () => {
    const responses = [
      sseResponse(['event: ready\ndata: {}\n\n', 'event: message\ndata: {"id":1}\n\nevent: del', 'eted\ndata: {"id":1}\n\n']),
      sseResponse(['event: ready\ndata: {}\n\n'], { hang: true })
    ]
    const fetchImpl = vi.fn(() => Promise.resolve(responses.shift()))
    const timers = []
    const events = []
    const statuses = []
    const onCatchUp = vi.fn()
    const stream = createChannelStream({
      url: '/api/channels/public/stream',
      headers: { 'X-Fyr-Channel-Key': 'k' },
      onEvent: (name, data) => events.push([name, data]),
      onStatus: (s, d) => statuses.push([s, d]),
      onCatchUp,
      fetchImpl,
      setTimer: (fn, ms) => timers.push({ fn, ms }),
      random: () => 1
    })
    stream.start()
    await flush()
    expect(events).toEqual([['message', { id: 1 }], ['deleted', { id: 1 }]])
    expect(onCatchUp).toHaveBeenCalledTimes(1)
    expect(fetchImpl.mock.calls[0][1].headers['X-Fyr-Channel-Key']).toBe('k')

    // Stream ended → reconnect scheduled after backoff; second connect catches up again.
    expect(timers).toHaveLength(1)
    expect(timers[0].ms).toBe(1000)
    timers.shift().fn()
    await flush()
    expect(onCatchUp).toHaveBeenCalledTimes(2)
    expect(statuses.map((s) => s[0])).toContain('live')
    stream.stop()
    expect(statuses.at(-1)[0]).toBe('stopped')
  })

  it('backs off on network errors and 5xx and resets after ready', async () => {
    const responses = [
      Promise.reject(new Error('offline')),
      Promise.resolve(sseResponse([], { status: 503 })),
      Promise.resolve(sseResponse(['event: ready\ndata: {}\n\n']))
    ]
    const timers = []
    const stream = createChannelStream({
      url: '/s',
      onEvent: () => {},
      fetchImpl: () => responses.shift() || new Promise(() => {}),
      setTimer: (fn, ms) => timers.push({ fn, ms }),
      random: () => 1
    })
    stream.start()
    await flush()
    expect(timers.at(-1).ms).toBe(1000)
    timers.shift().fn()
    await flush()
    expect(timers.at(-1).ms).toBe(2000)
    timers.shift().fn()
    await flush()
    // Connected (ready) then ended → attempt was reset.
    expect(timers.at(-1).ms).toBe(1000)
    stream.stop()
  })

  it('stops on terminal statuses and on channel deletion', async () => {
    for (const status of [401, 403, 404]) {
      const statuses = []
      const timers = []
      createChannelStream({
        url: '/s',
        onEvent: () => {},
        onStatus: (s, d) => statuses.push([s, d]),
        fetchImpl: () => Promise.resolve(sseResponse([], { status })),
        setTimer: (fn, ms) => timers.push({ fn, ms })
      }).start()
      await flush()
      expect(statuses.at(-1)).toEqual(['error', { status }])
      expect(timers).toHaveLength(0)
    }

    const timers = []
    const events = []
    createChannelStream({
      url: '/s',
      onEvent: (n) => events.push(n),
      fetchImpl: () => Promise.resolve(sseResponse(['event: channel_deleted\ndata: {}\n\nevent: message\ndata: {}\n\n'])),
      setTimer: (fn, ms) => timers.push({ fn, ms })
    }).start()
    await flush()
    expect(events).toEqual(['channel_deleted'])
    expect(timers).toHaveLength(0)
  })

  it('stop() aborts the request and cancels pending reconnects', async () => {
    let signal
    const cleared = []
    const timers = []
    const stream = createChannelStream({
      url: '/s',
      onEvent: () => {},
      fetchImpl: (_url, init) => {
        signal = init.signal
        return Promise.reject(new Error('x'))
      },
      setTimer: (fn, ms) => {
        timers.push(fn)
        return 42
      },
      clearTimer: (id) => cleared.push(id)
    })
    stream.start()
    await flush()
    stream.stop()
    expect(signal.aborted).toBe(true)
    expect(cleared).toEqual([42])
  })
})
