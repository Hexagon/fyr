// Pure formatting helpers for messages. Message text is never rendered as
// HTML: `linkify` splits it into text and link segments that the template
// renders with text interpolation, so markup in messages stays inert.

import { locale, t } from '../../i18n/index.js'

const URL_PATTERN = /\bhttps?:\/\/[^\s<>"'`]+/gi
const TRAILING_PUNCTUATION = /[.,;:!?'"\]}>]+$/

export const CATEGORIES = Object.freeze(['info', 'need', 'offer', 'alert'])
/** Translated label for a message category ('info', 'need', …). */
export const categoryLabel = (category) => (CATEGORIES.includes(category) ? t(`category.${category}`) : String(category ?? ''))

const trimUrl = (raw) => {
  let url = raw.replace(TRAILING_PUNCTUATION, '')
  // Drop an unbalanced closing parenthesis, e.g. "(see https://x.org)".
  while (url.endsWith(')') && (url.match(/\(/g) || []).length < (url.match(/\)/g) || []).length) {
    url = url.slice(0, -1).replace(TRAILING_PUNCTUATION, '')
  }
  return url
}

const safeHref = (candidate) => {
  try {
    const parsed = new URL(candidate)
    return parsed.protocol === 'http:' || parsed.protocol === 'https:' ? parsed.href : null
  } catch {
    return null
  }
}

/** Split text into `{ type: 'text' | 'link', text, href? }` segments. */
export const linkify = (input) => {
  const text = String(input ?? '')
  const segments = []
  let last = 0
  for (const match of text.matchAll(URL_PATTERN)) {
    const url = trimUrl(match[0])
    const href = safeHref(url)
    if (!href) continue
    if (match.index > last) segments.push({ type: 'text', text: text.slice(last, match.index) })
    segments.push({ type: 'link', text: url, href })
    last = match.index + url.length
  }
  if (last < text.length) segments.push({ type: 'text', text: text.slice(last) })
  return segments
}

/** Stable colour for a device tag (display only). */
export const deviceColor = (tag) => {
  const value = parseInt(String(tag || '0').slice(0, 6), 16)
  const hue = Number.isFinite(value) ? value % 360 : 0
  return `hsl(${hue}, 70%, 62%)`
}

const pad = (n) => String(n).padStart(2, '0')

export const dayKey = (ts) => {
  const d = new Date(ts)
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

export const dayLabel = (ts, now = Date.now()) => {
  const key = dayKey(ts)
  if (key === dayKey(now)) return t('time.today')
  if (key === dayKey(now - 86400000)) return t('time.yesterday')
  return new Date(ts).toLocaleDateString(locale(), { weekday: 'short', year: 'numeric', month: 'short', day: 'numeric' })
}

export const timeLabel = (ts) => {
  const d = new Date(ts)
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** Group ascending messages by local calendar day. */
export const groupByDay = (messages, now = Date.now()) => {
  const groups = []
  for (const message of messages) {
    const key = dayKey(message.created_at)
    let group = groups[groups.length - 1]
    if (!group || group.key !== key) {
      group = { key, label: dayLabel(message.created_at, now), messages: [] }
      groups.push(group)
    }
    group.messages.push(message)
  }
  return groups
}

const firstLine = (text, max = 60) => {
  const line = String(text ?? '').split('\n')[0].trim()
  return line.length > max ? `${line.slice(0, max - 1)}…` : line
}

/** Deep link to the Maps page for a message location. */
export const mapsLink = ({ lat, lng, label }) => {
  const params = new URLSearchParams({ lat: String(lat), lng: String(lng), zoom: '15' })
  const title = firstLine(label)
  if (title) params.set('label', title)
  return `/maps?${params.toString()}`
}

export const formatCoords = (lat, lng) => `${Number(lat).toFixed(5)}, ${Number(lng).toFixed(5)}`

export { firstLine }

/** Count characters the way the server does (Unicode scalar values). */
export const charCount = (text) => [...String(text ?? '')].length
