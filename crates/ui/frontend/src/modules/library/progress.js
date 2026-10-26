// Per-browser reading progress and bookmarks.
//
// Stored in localStorage (not on the server) because guests in password mode
// have no server-side writes. All storage access goes through safeStorage, so
// a missing, blocked, full or corrupted store degrades to an in-memory copy
// for the current page session instead of throwing.
//
// Layout (key `fyr.library.progress.v1`):
//   { v: 1, books: [ { filename, title, format, lastOpened, updatedAt,
//                      position, bookmarks: [ { id, title, position, createdAt } ] } ] }
//
// Books are kept in an array (not an object keyed by filename) so filenames
// like "__proto__" or "constructor" cannot collide with Object prototype keys.

import { getDefaultStorage, readJson, writeJson } from './safeStorage'

export const PROGRESS_STORAGE_KEY = 'fyr.library.progress.v1'
export const DEFAULT_MAX_BOOKS = 200
export const DEFAULT_MAX_BOOKMARKS = 100
export const PROGRESS_EXPORT_FORMAT = 'fyr-reading-progress'

const MAX_FILENAME = 1024
const MAX_TEXT = 200
const MAX_LOCATOR = 4096
const POSITION_TYPES = new Set(['epub', 'zim', 'md', 'pdf'])

const clipText = (value, max = MAX_TEXT) => {
  if (typeof value !== 'string') return ''
  const trimmed = value.trim()
  return trimmed.length > max ? trimmed.slice(0, max) : trimmed
}

const finiteNumber = (value) => (typeof value === 'number' && Number.isFinite(value) ? value : null)

const isValidFilename = (filename) =>
  typeof filename === 'string' && filename.length > 0 && filename.length <= MAX_FILENAME

/**
 * Normalize a reading position. Returns null for anything unusable so corrupt
 * or hostile stored data never reaches the readers.
 */
export const normalizePosition = (position) => {
  if (!position || typeof position !== 'object') return null
  const type = position.type
  if (!POSITION_TYPES.has(type)) return null

  const out = { type }
  const label = clipText(position.label)
  if (label) out.label = label

  if (type === 'epub') {
    if (typeof position.cfi !== 'string' || !position.cfi || position.cfi.length > MAX_LOCATOR) return null
    out.cfi = position.cfi
    if (typeof position.href === 'string' && position.href.length <= MAX_LOCATOR && position.href) {
      out.href = position.href
    }
    return out
  }

  if (type === 'zim') {
    if (typeof position.path !== 'string' || !position.path || position.path.length > MAX_LOCATOR) return null
    out.path = position.path
    return out
  }

  if (type === 'md') {
    const line = finiteNumber(position.line)
    const fraction = finiteNumber(position.fraction)
    if (line === null && fraction === null) return null
    if (line !== null) out.line = Math.max(1, Math.floor(line))
    if (fraction !== null) out.fraction = Math.min(1, Math.max(0, fraction))
    return out
  }

  // pdf: 1-based page plus how far down that page the viewport top is (0..1)
  const page = finiteNumber(position.page)
  if (page === null || page < 1) return null
  out.page = Math.floor(page)
  const offset = finiteNumber(position.offset)
  if (offset !== null) out.offset = Math.round(Math.min(1, Math.max(0, offset)) * 10000) / 10000
  return out
}

/** Stable identity of a position, used to de-duplicate bookmarks. */
export const positionKey = (position) => {
  const pos = normalizePosition(position)
  if (!pos) return ''
  if (pos.type === 'epub') return `epub:${pos.cfi}`
  if (pos.type === 'zim') return `zim:${pos.path}`
  if (pos.type === 'md') return pos.line ? `md:line:${pos.line}` : `md:frac:${pos.fraction.toFixed(3)}`
  // One bookmark per PDF page: re-bookmarking further down a page renames it.
  return `pdf:${pos.page}`
}

const normalizeBookmark = (bookmark) => {
  if (!bookmark || typeof bookmark !== 'object') return null
  const position = normalizePosition(bookmark.position)
  if (!position) return null
  const id = clipText(bookmark.id, 64)
  if (!id) return null
  return {
    id,
    title: clipText(bookmark.title) || position.label || 'Bookmark',
    position,
    createdAt: finiteNumber(bookmark.createdAt) ?? 0
  }
}

const normalizeEntry = (entry, maxBookmarks) => {
  if (!entry || typeof entry !== 'object' || !isValidFilename(entry.filename)) return null
  const bookmarks = Array.isArray(entry.bookmarks)
    ? entry.bookmarks.map(normalizeBookmark).filter(Boolean).slice(-maxBookmarks)
    : []
  return {
    filename: entry.filename,
    title: clipText(entry.title),
    format: clipText(entry.format, 16),
    lastOpened: finiteNumber(entry.lastOpened) ?? 0,
    updatedAt: finiteNumber(entry.updatedAt) ?? 0,
    position: normalizePosition(entry.position),
    bookmarks
  }
}

const recency = (entry) => Math.max(entry.lastOpened || 0, entry.updatedAt || 0)

const emptyState = () => ({ v: 1, books: [] })

const normalizeState = (raw, maxBooks, maxBookmarks) => {
  if (!raw || typeof raw !== 'object' || !Array.isArray(raw.books)) return emptyState()
  const seen = new Set()
  const books = []
  for (const candidate of raw.books) {
    const entry = normalizeEntry(candidate, maxBookmarks)
    if (!entry || seen.has(entry.filename)) continue
    seen.add(entry.filename)
    books.push(entry)
  }
  books.sort((a, b) => recency(b) - recency(a))
  return { v: 1, books: books.slice(0, maxBooks) }
}

const cloneEntry = (entry) => (entry ? JSON.parse(JSON.stringify(entry)) : null)

export const createProgressStore = ({
  storage,
  key = PROGRESS_STORAGE_KEY,
  maxBooks = DEFAULT_MAX_BOOKS,
  maxBookmarks = DEFAULT_MAX_BOOKMARKS,
  now = () => Date.now()
} = {}) => {
  // `storage === undefined` means "use window.localStorage, resolved lazily".
  const resolveStorage = () => (storage === undefined ? getDefaultStorage() : storage)

  // In-memory mirror used when persistence is unavailable or a write failed.
  let memory = emptyState()
  let storageHealthy = true

  const read = () => {
    const backend = resolveStorage()
    if (!backend || !storageHealthy) return normalizeState(memory, maxBooks, maxBookmarks)
    const raw = readJson(backend, key, null)
    const state = normalizeState(raw, maxBooks, maxBookmarks)
    memory = state
    return state
  }

  const write = (state) => {
    const normalized = normalizeState(state, maxBooks, maxBookmarks)
    memory = normalized
    const backend = resolveStorage()
    if (!backend) {
      storageHealthy = false
      return false
    }
    if (writeJson(backend, key, normalized)) {
      storageHealthy = true
      return true
    }
    // Probably QuotaExceededError: drop the older half and retry once.
    const trimmed = { v: 1, books: normalized.books.slice(0, Math.max(1, Math.floor(normalized.books.length / 2))) }
    if (writeJson(backend, key, trimmed)) {
      memory = trimmed
      storageHealthy = true
      return true
    }
    storageHealthy = false
    return false
  }

  const update = (filename, mutate) => {
    if (!isValidFilename(filename)) return null
    const state = read()
    let entry = state.books.find((book) => book.filename === filename)
    if (!entry) {
      entry = normalizeEntry({ filename }, maxBookmarks)
      state.books.push(entry)
    }
    const result = mutate(entry)
    write(state)
    return result
  }

  const getEntry = (filename) => {
    if (!isValidFilename(filename)) return null
    return cloneEntry(read().books.find((book) => book.filename === filename) || null)
  }

  const touchBook = (filename, meta = {}) =>
    update(filename, (entry) => {
      entry.lastOpened = now()
      const title = clipText(meta.title)
      const format = clipText(meta.format, 16)
      if (title) entry.title = title
      if (format) entry.format = format
      return true
    })

  const savePosition = (filename, position) => {
    const normalized = normalizePosition(position)
    if (!normalized) return false
    return Boolean(
      update(filename, (entry) => {
        entry.position = normalized
        entry.updatedAt = now()
        return true
      })
    )
  }

  const getPosition = (filename) => getEntry(filename)?.position || null

  const listRecent = (limit = 10) =>
    read()
      .books.filter((book) => book.lastOpened > 0)
      .sort((a, b) => b.lastOpened - a.lastOpened)
      .slice(0, Math.max(0, limit))
      .map(cloneEntry)

  const getBookmarks = (filename) => {
    const entry = getEntry(filename)
    if (!entry) return []
    return entry.bookmarks.slice().sort((a, b) => a.createdAt - b.createdAt)
  }

  const addBookmark = (filename, { title, position } = {}) => {
    const normalized = normalizePosition(position)
    if (!normalized) return null
    const keyOfNew = positionKey(normalized)
    return update(filename, (entry) => {
      const existing = entry.bookmarks.find((bookmark) => positionKey(bookmark.position) === keyOfNew)
      if (existing) {
        const nextTitle = clipText(title)
        if (nextTitle) existing.title = nextTitle
        return { ...existing }
      }
      const createdAt = now()
      const bookmark = {
        id: `${createdAt.toString(36)}-${Math.random().toString(36).slice(2, 10)}`,
        title: clipText(title) || normalized.label || 'Bookmark',
        position: normalized,
        createdAt
      }
      entry.bookmarks.push(bookmark)
      if (entry.bookmarks.length > maxBookmarks) {
        entry.bookmarks.splice(0, entry.bookmarks.length - maxBookmarks)
      }
      entry.updatedAt = createdAt
      return { ...bookmark }
    })
  }

  const removeBookmark = (filename, id) => {
    if (!isValidFilename(filename) || !id) return false
    const state = read()
    const entry = state.books.find((book) => book.filename === filename)
    if (!entry) return false
    const before = entry.bookmarks.length
    entry.bookmarks = entry.bookmarks.filter((bookmark) => bookmark.id !== id)
    if (entry.bookmarks.length === before) return false
    write(state)
    return true
  }

  const forgetBook = (filename) => {
    const state = read()
    const next = state.books.filter((book) => book.filename !== filename)
    if (next.length === state.books.length) return false
    write({ v: 1, books: next })
    return true
  }

  const clearAll = () => write(emptyState())

  /** Portable snapshot of all progress + bookmarks (for moving devices). */
  const exportData = () => ({
    format: PROGRESS_EXPORT_FORMAT,
    v: 1,
    exported_at: now(),
    books: read().books.map(cloneEntry)
  })

  /**
   * Merge an exported snapshot into this store. Never deletes anything:
   * - unknown books are added;
   * - for known books the newer position (by `updatedAt`) wins, `lastOpened`
   *   takes the later value and missing title/format are filled in;
   * - bookmarks are united, de-duplicated by position (local titles win).
   * Everything is normalized first, so hostile files cannot inject bad data.
   * @returns {{ booksAdded: number, booksUpdated: number, bookmarksAdded: number }}
   */
  const importData = (raw) => {
    if (!raw || typeof raw !== 'object' || raw.format !== PROGRESS_EXPORT_FORMAT || raw.v !== 1) {
      throw new Error('Not a Fyr reading progress file')
    }
    const incoming = normalizeState(raw, Number.MAX_SAFE_INTEGER, maxBookmarks).books
    const state = read()
    const result = { booksAdded: 0, booksUpdated: 0, bookmarksAdded: 0 }
    for (const other of incoming) {
      const mine = state.books.find((book) => book.filename === other.filename)
      if (!mine) {
        state.books.push(other)
        result.booksAdded += 1
        result.bookmarksAdded += other.bookmarks.length
        continue
      }
      let changed = false
      if (other.position && (!mine.position || other.updatedAt > mine.updatedAt)) {
        mine.position = other.position
        mine.updatedAt = Math.max(mine.updatedAt, other.updatedAt)
        changed = true
      }
      if (other.lastOpened > mine.lastOpened) {
        mine.lastOpened = other.lastOpened
        changed = true
      }
      if (!mine.title && other.title) {
        mine.title = other.title
        changed = true
      }
      if (!mine.format && other.format) {
        mine.format = other.format
        changed = true
      }
      const keys = new Set(mine.bookmarks.map((bookmark) => positionKey(bookmark.position)))
      const ids = new Set(mine.bookmarks.map((bookmark) => bookmark.id))
      for (const bookmark of other.bookmarks) {
        const key = positionKey(bookmark.position)
        if (keys.has(key)) continue
        keys.add(key)
        mine.bookmarks.push(ids.has(bookmark.id) ? { ...bookmark, id: `${bookmark.id}-i` } : bookmark)
        ids.add(bookmark.id)
        result.bookmarksAdded += 1
        changed = true
      }
      if (changed) {
        mine.bookmarks.sort((a, b) => a.createdAt - b.createdAt)
        if (mine.bookmarks.length > maxBookmarks) mine.bookmarks.splice(0, mine.bookmarks.length - maxBookmarks)
        result.booksUpdated += 1
      }
    }
    write(state)
    return result
  }

  return {
    exportData,
    importData,
    getEntry,
    touchBook,
    savePosition,
    getPosition,
    listRecent,
    getBookmarks,
    addBookmark,
    removeBookmark,
    forgetBook,
    clearAll
  }
}

/** Shared store backed by window.localStorage. */
export const progressStore = createProgressStore()
