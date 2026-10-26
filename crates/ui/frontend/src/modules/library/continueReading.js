// "Continue reading" selection for the Home page: recently opened books from
// progress.js, restricted to files that are still in the library listing.

import { buildBookDeepLink } from './deepLink'

const formatOf = (filename) => {
  const idx = String(filename || '').lastIndexOf('.')
  return idx >= 0 ? filename.slice(idx + 1).toLowerCase() : ''
}

/**
 * @param recent  entries from progressStore.listRecent() (newest first)
 * @param books   library listing from apiService.getBooks() ({ filename, title })
 * @returns [{ filename, title, format, lastOpened, positionLabel, bookmarkCount, href }]
 */
export const selectContinueReading = (recent, books, limit = 6) => {
  if (!Array.isArray(recent) || !Array.isArray(books)) return []
  const present = new Map()
  for (const book of books) {
    if (book && typeof book.filename === 'string') present.set(book.filename, book)
  }

  const out = []
  for (const entry of recent) {
    if (out.length >= limit) break
    const book = entry && present.get(entry.filename)
    if (!book) continue
    out.push({
      filename: entry.filename,
      title: book.title || entry.title || entry.filename.replace(/\.[^/.]+$/, ''),
      format: (entry.format || formatOf(entry.filename)).toUpperCase(),
      lastOpened: entry.lastOpened || 0,
      positionLabel: entry.position?.label || '',
      bookmarkCount: Array.isArray(entry.bookmarks) ? entry.bookmarks.length : 0,
      href: buildBookDeepLink(entry.filename)
    })
  }
  return out
}

/** Short relative time ("just now", "5 min ago", "3 days ago"). */
export const formatRelativeTime = (timestamp, now = Date.now()) => {
  if (!timestamp) return ''
  const diff = Math.max(0, now - timestamp)
  const minute = 60 * 1000
  const hour = 60 * minute
  const day = 24 * hour
  if (diff < minute) return 'just now'
  if (diff < hour) return `${Math.floor(diff / minute)} min ago`
  if (diff < day) {
    const h = Math.floor(diff / hour)
    return `${h} hour${h === 1 ? '' : 's'} ago`
  }
  const d = Math.floor(diff / day)
  if (d < 30) return `${d} day${d === 1 ? '' : 's'} ago`
  return new Date(timestamp).toLocaleDateString()
}
