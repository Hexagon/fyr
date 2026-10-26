// Client-side in-book search for PDFs, over pdf.js text content.
//
// The server's /api/library/books/:f/search does not read PDFs (that would
// need a PDF text extractor in Rust), so the browser searches the text pdf.js
// already extracts for the text layer. Matching ignores case and accents (and
// folds ligatures such as "ﬁ"), like the global search, and treats any run of
// whitespace as one space.
//
// A page's text is the concatenation of its text items (the `str` of every
// item that has one, in order, i.e. exactly the spans of the pdf.js
// TextLayer), with "\n" after items that end a line. Keeping `starts` (the
// offset of every item in that text) lets a match be mapped back onto the
// text-layer spans for highlighting.

export const DEFAULT_MAX_RESULTS = 200
export const SNIPPET_RADIUS = 60

const COMBINING_MARKS = /\p{M}/gu
const WHITESPACE = /\s/

/**
 * Fold `text` for matching. Returns { folded, starts, ends } where folded[i]
 * came from text.slice(starts[i], ends[i]).
 */
export const foldWithMap = (text) => {
  const source = String(text ?? '')
  let folded = ''
  const starts = []
  const ends = []
  let lastWasSpace = true // also trims leading whitespace
  let i = 0
  while (i < source.length) {
    const code = source.codePointAt(i)
    const char = String.fromCodePoint(code)
    const next = i + char.length
    if (WHITESPACE.test(char)) {
      if (!lastWasSpace) {
        folded += ' '
        starts.push(i)
        ends.push(next)
        lastWasSpace = true
      }
    } else {
      const out = char.normalize('NFKD').replace(COMBINING_MARKS, '').toLowerCase()
      for (const piece of out) {
        // Compatibility decompositions can produce spaces (e.g. U+00A0).
        if (WHITESPACE.test(piece)) {
          if (lastWasSpace) continue
          folded += ' '
          lastWasSpace = true
        } else {
          folded += piece
          lastWasSpace = false
        }
        // A piece may be a surrogate pair; keep one map entry per UTF-16 unit.
        for (let k = 0; k < piece.length; k += 1) {
          starts.push(i)
          ends.push(next)
        }
      }
    }
    i = next
  }
  return { folded, starts, ends }
}

/** Folded form of a query (or '' when there is nothing to search for). */
export const foldQuery = (query) => foldWithMap(query).folded.trim()

/**
 * Turn pdf.js text content (`{ items }`) into { text, strs, starts }.
 * `strs` is parallel to the pdf.js TextLayer's `textDivs`.
 */
export const buildPageText = (textContent) => {
  const items = Array.isArray(textContent?.items) ? textContent.items : []
  const strs = []
  const starts = []
  let text = ''
  for (const item of items) {
    if (!item || typeof item.str !== 'string') continue
    starts.push(text.length)
    strs.push(item.str)
    text += item.str
    if (item.hasEOL) text += '\n'
  }
  return { text, strs, starts }
}

/**
 * Non-overlapping matches of `query` in `text`, as [start, end) offsets into
 * the original text.
 */
export const findMatches = (text, query, limit = Infinity) => {
  const needle = typeof query === 'string' ? foldQuery(query) : ''
  if (!needle) return []
  const { folded, starts, ends } = foldWithMap(text)
  const matches = []
  let from = 0
  while (matches.length < limit) {
    const at = folded.indexOf(needle, from)
    if (at < 0) break
    const last = at + needle.length - 1
    matches.push({ start: starts[at], end: ends[last] })
    from = at + needle.length
  }
  return matches
}

const collapse = (text) => text.replace(/\s+/g, ' ')

/** Short context around [start, end), whitespace collapsed, with ellipses. */
export const snippetFor = (text, start, end, radius = SNIPPET_RADIUS) => {
  const source = String(text || '')
  let from = Math.max(0, start - radius)
  let to = Math.min(source.length, end + radius)
  // Do not cut words in half where we can avoid it.
  if (from > 0) {
    const space = source.slice(from, start).search(/\s/)
    if (space >= 0 && space < radius / 2) from += space + 1
  }
  if (to < source.length) {
    const tail = source.slice(end, to)
    const space = tail.search(/\s[^\s]*$/)
    if (space > radius / 2) to = end + space
  }
  const body = collapse(source.slice(from, to)).trim()
  return `${from > 0 ? '… ' : ''}${body}${to < source.length ? ' …' : ''}`
}

/** Result entries for one page, in the shape the Books result list uses. */
export const pageResults = (page, text, query, limit = Infinity) =>
  findMatches(text, query, limit).map((match, occurrence) => ({
    path: `page:${page}:${occurrence}`,
    title: `Page ${page}`,
    page,
    occurrence,
    snippet: snippetFor(text, match.start, match.end)
  }))

/**
 * Search the whole document. `getPageText(n)` resolves to a buildPageText()
 * result. Pages are searched in order; stops at `maxResults` matches or when
 * `isCancelled()` turns true (then resolves to null). `onProgress(page,
 * pageCount)` is called after each page.
 */
export const searchPdfText = async ({
  pageCount,
  getPageText,
  query,
  maxResults = DEFAULT_MAX_RESULTS,
  isCancelled = () => false,
  onProgress = () => {}
}) => {
  const needle = foldQuery(query)
  const results = []
  let truncated = false
  if (!needle) return { results, truncated }
  const count = Math.max(0, Math.floor(Number(pageCount) || 0))
  for (let page = 1; page <= count; page += 1) {
    let pageText
    try {
      pageText = await getPageText(page)
    } catch {
      pageText = null // a damaged page should not abort the whole search
    }
    if (isCancelled()) return null
    if (pageText?.text) {
      const remaining = maxResults - results.length
      const found = pageResults(page, pageText.text, query, remaining + 1)
      if (found.length > remaining) {
        results.push(...found.slice(0, remaining))
        truncated = true
        onProgress(page, count)
        break
      }
      results.push(...found)
    }
    onProgress(page, count)
  }
  return { results, truncated }
}

/**
 * Split text-layer items into highlight segments. `strs`/`starts` come from
 * buildPageText, `matches` from findMatches on the same text, and `selected`
 * is the index of the current match (or -1).
 *
 * Returns a Map: item index -> [{ text, match: index|-1 }]; items without a
 * match are not in the map.
 */
export const highlightSegments = (strs, starts, matches, selected = -1) => {
  const segments = new Map()
  if (!Array.isArray(strs) || !Array.isArray(matches) || !matches.length) return segments
  let item = 0
  matches.forEach((match, matchIndex) => {
    // First item that ends after the match start.
    while (item < strs.length && starts[item] + strs[item].length <= match.start) item += 1
    for (let i = item; i < strs.length && starts[i] < match.end; i += 1) {
      const itemStart = starts[i]
      const itemEnd = itemStart + strs[i].length
      const from = Math.max(match.start, itemStart) - itemStart
      const to = Math.min(match.end, itemEnd) - itemStart
      if (to <= from) continue
      const list = segments.get(i) || []
      list.push({ from, to, match: matchIndex })
      segments.set(i, list)
    }
  })

  const out = new Map()
  for (const [i, ranges] of segments) {
    const str = strs[i]
    const parts = []
    let cursor = 0
    for (const range of ranges) {
      if (range.from > cursor) parts.push({ text: str.slice(cursor, range.from), match: -1 })
      parts.push({
        text: str.slice(range.from, range.to),
        match: range.match,
        selected: range.match === selected
      })
      cursor = range.to
    }
    if (cursor < str.length) parts.push({ text: str.slice(cursor), match: -1 })
    out.set(i, parts)
  }
  return out
}
