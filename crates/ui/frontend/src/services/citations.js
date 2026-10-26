// Library sources and [n] citations for the assistant ("ask your library").
//
// The server emits a `sources` SSE event with `[{ n, title, source, link }]`
// before the answer tokens. The model is told to cite sources inline as
// `[n]`; `linkCitations` turns those markers into in-app links to the Books
// reader while leaving code spans and unknown numbers untouched.

const MAX_SOURCE_NUMBER = 99

const escapeHtml = (value) => String(value ?? '')
  .replace(/&/g, '&amp;')
  .replace(/</g, '&lt;')
  .replace(/>/g, '&gt;')
  .replace(/"/g, '&quot;')
  .replace(/'/g, '&#39;')

// Only same-origin, path-absolute links are accepted (no `//host`, no
// `javascript:`), so a malformed payload can never navigate off the app.
const isSafeInternalLink = (link) => typeof link === 'string' && link.startsWith('/') && !link.startsWith('//') && !link.includes('\\')

export const normalizeSources = (raw) => {
  if (!Array.isArray(raw)) return []
  const seen = new Set()
  const out = []
  for (const item of raw) {
    if (!item || typeof item !== 'object') continue
    const n = Number(item.n)
    if (!Number.isInteger(n) || n < 1 || n > MAX_SOURCE_NUMBER || seen.has(n)) continue
    const source = typeof item.source === 'string' ? item.source : ''
    const title = typeof item.title === 'string' && item.title.trim() ? item.title.trim() : source || `Source ${n}`
    seen.add(n)
    out.push({
      n,
      title,
      source,
      link: isSafeInternalLink(item.link) ? item.link : ''
    })
  }
  return out.sort((a, b) => a.n - b.n)
}

// Parse the `data` of a `sources` SSE event. Malformed payloads yield [].
export const parseSourcesEvent = (data) => {
  try {
    return normalizeSources(JSON.parse(String(data ?? '')))
  } catch {
    return []
  }
}

// `[1]`, `[1, 2]`, `[1;3]` — but not markdown links `[1](...)` or reference
// definitions `[1]: ...`.
const CITATION_PATTERN = /\[(\d{1,2}(?:\s*[,;]\s*\d{1,2})*)\](?![(:])/g

const renderCitationGroup = (numbers, byNumber) => numbers
  .map((n) => {
    const source = byNumber.get(n)
    if (!source) return `[${n}]`
    const label = `&#91;${n}&#93;`
    const title = escapeHtml(source.source ? `${source.title} — ${source.source}` : source.title)
    if (!source.link) {
      return `<span class="citation" data-citation="${n}" title="${title}">${label}</span>`
    }
    return `<a class="citation" href="${escapeHtml(source.link)}" data-citation="${n}" title="${title}">${label}</a>`
  })
  .join('')

const linkCitationsInText = (text, byNumber) => text.replace(CITATION_PATTERN, (match, group) => {
  const numbers = group.split(/[,;]/).map((part) => Number(part.trim()))
  if (!numbers.some((n) => byNumber.has(n))) return match
  return renderCitationGroup(numbers, byNumber)
})

// Replace `[n]` citations in markdown with links to their sources. Fenced code
// blocks and inline code spans are left alone.
export const linkCitations = (markdown, sources) => {
  const text = String(markdown ?? '')
  const list = normalizeSources(sources)
  if (!list.length || !text.includes('[')) return text
  const byNumber = new Map(list.map((source) => [source.n, source]))

  let inFence = false
  let fenceMarker = ''
  return text
    .split('\n')
    .map((line) => {
      const fence = /^\s*(```|~~~)/.exec(line)
      if (fence) {
        if (!inFence) {
          inFence = true
          fenceMarker = fence[1]
        } else if (fence[1] === fenceMarker) {
          inFence = false
        }
        return line
      }
      if (inFence) return line
      // Alternate between prose and inline code spans.
      return line
        .split(/(`+[^`]*`+)/)
        .map((part, index) => (index % 2 === 1 ? part : linkCitationsInText(part, byNumber)))
        .join('')
    })
    .join('\n')
}

// Numbers cited in an answer (ignoring code), e.g. to highlight used sources.
export const citedNumbers = (markdown, sources) => {
  const linked = linkCitations(markdown, sources)
  const out = new Set()
  for (const match of linked.matchAll(/data-citation="(\d+)"/g)) {
    out.add(Number(match[1]))
  }
  return out
}
