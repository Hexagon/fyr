// Resilient SSE client for message channels.
//
// EventSource cannot send custom headers, and encrypted channels need the
// X-Fyr-Channel-Key header, so streams are read with fetch(). The server
// sends `ready` first; on every (re)connect we then call `onCatchUp()` so
// the caller can fetch `?since=<last id>` and fill any gap. Reconnects use
// exponential backoff with jitter; 400/401/403/404 are terminal.

import { extractNextSseFrame, parseSseFrame } from '../../services/api.js'

export const TERMINAL_STATUSES = new Set([400, 401, 403, 404])

/** Exponential backoff with jitter: base * 2^attempt, capped, scaled 50-100%. */
export const backoffDelay = (attempt, { baseMs = 1000, maxMs = 30000 } = {}, random = Math.random) => {
  const raw = Math.min(maxMs, baseMs * 2 ** Math.max(0, attempt))
  return Math.round(raw * (0.5 + random() * 0.5))
}

/**
 * @param {object} options
 * @param {string} options.url
 * @param {Record<string,string>} [options.headers]
 * @param {(name: string, data: any) => void} options.onEvent
 * @param {(status: 'connecting'|'live'|'reconnecting'|'stopped'|'error', detail?: any) => void} [options.onStatus]
 * @param {() => Promise<void>|void} [options.onCatchUp]
 */
export const createChannelStream = ({
  url,
  headers = {},
  onEvent,
  onStatus = () => {},
  onCatchUp = () => {},
  fetchImpl = (...args) => globalThis.fetch(...args),
  setTimer = (fn, ms) => setTimeout(fn, ms),
  clearTimer = (id) => clearTimeout(id),
  random = Math.random,
  backoff = {}
}) => {
  let attempt = 0
  let stopped = false
  let controller = null
  let timer = null

  const scheduleReconnect = () => {
    if (stopped) return
    const delay = backoffDelay(attempt, backoff, random)
    attempt += 1
    onStatus('reconnecting', { delay, attempt })
    timer = setTimer(connect, delay)
  }

  const dispatch = async (frame) => {
    const { event, data } = parseSseFrame(frame)
    let parsed = data
    try {
      parsed = data ? JSON.parse(data) : null
    } catch {
      // keep raw string
    }
    if (event === 'ready') {
      attempt = 0
      onStatus('live')
      try {
        await onCatchUp()
      } catch {
        // A failed catch-up is retried on the next reconnect.
      }
      return
    }
    onEvent(event, parsed)
    if (event === 'channel_deleted') {
      stopped = true
      onStatus('error', { status: 404 })
    }
  }

  async function connect() {
    if (stopped) return
    timer = null
    controller = new AbortController()
    onStatus(attempt === 0 ? 'connecting' : 'reconnecting', { attempt })
    let response
    try {
      response = await fetchImpl(url, {
        headers: { Accept: 'text/event-stream', ...headers },
        signal: controller.signal,
        cache: 'no-store'
      })
    } catch {
      scheduleReconnect()
      return
    }
    if (!response.ok || !response.body) {
      if (TERMINAL_STATUSES.has(response.status)) {
        stopped = true
        onStatus('error', { status: response.status })
        return
      }
      scheduleReconnect()
      return
    }
    const reader = response.body.getReader()
    const decoder = new TextDecoder()
    let buffer = ''
    try {
      for (;;) {
        const { value, done } = await reader.read()
        if (done) break
        buffer += decoder.decode(value, { stream: true }).replace(/\r\n/g, '\n')
        let next
        while ((next = extractNextSseFrame(buffer))) {
          buffer = next.remainder
          if (next.frame.trim()) await dispatch(next.frame)
          if (stopped) return
        }
      }
    } catch {
      // Network drop or abort.
    }
    scheduleReconnect()
  }

  return {
    start() {
      stopped = false
      attempt = 0
      connect()
    },
    stop() {
      stopped = true
      if (timer) clearTimer(timer)
      timer = null
      controller?.abort()
      onStatus('stopped')
    },
    /** Reconnect immediately (e.g. when the browser comes back online). */
    kick() {
      if (stopped || !timer) return
      clearTimer(timer)
      attempt = 0
      connect()
    }
  }
}
