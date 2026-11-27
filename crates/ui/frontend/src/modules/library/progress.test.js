import { describe, expect, it } from 'vitest'
import { PROGRESS_STORAGE_KEY, createProgressStore, normalizePosition, positionKey } from './progress'

const createMemoryStorage = (initial = {}) => {
  const data = new Map(Object.entries(initial))
  return {
    data,
    getItem: (key) => (data.has(key) ? data.get(key) : null),
    setItem: (key, value) => data.set(key, String(value)),
    removeItem: (key) => data.delete(key)
  }
}

const createClock = (start = 1_000) => {
  let t = start
  return () => {
    t += 1
    return t
  }
}

const epub = (cfi, label) => ({ type: 'epub', cfi, label })

describe('progress store: positions', () => {
  it('saves and restores a position per book', () => {
    const storage = createMemoryStorage()
    const store = createProgressStore({ storage, now: createClock() })
    expect(store.savePosition('a.epub', epub('epubcfi(/6/4!/4/2)', 'Chapter 1'))).toBe(true)
    expect(store.getPosition('a.epub')).toEqual({ type: 'epub', cfi: 'epubcfi(/6/4!/4/2)', label: 'Chapter 1' })

    // A second store instance reads what the first persisted.
    const again = createProgressStore({ storage })
    expect(again.getPosition('a.epub')?.cfi).toBe('epubcfi(/6/4!/4/2)')
  })

  it('isolates books from each other', () => {
    const store = createProgressStore({ storage: createMemoryStorage(), now: createClock() })
    store.savePosition('one.zim', { type: 'zim', path: 'A/One' })
    store.savePosition('two.zim', { type: 'zim', path: 'A/Two' })
    store.addBookmark('one.zim', { title: 'Mark', position: { type: 'zim', path: 'A/One#s' } })
    expect(store.getPosition('one.zim').path).toBe('A/One')
    expect(store.getPosition('two.zim').path).toBe('A/Two')
    expect(store.getBookmarks('one.zim')).toHaveLength(1)
    expect(store.getBookmarks('two.zim')).toEqual([])
  })

  it('handles filenames that collide with Object prototype keys', () => {
    const store = createProgressStore({ storage: createMemoryStorage(), now: createClock() })
    store.savePosition('__proto__', { type: 'md', line: 4 })
    store.savePosition('constructor', { type: 'md', line: 9 })
    expect(store.getPosition('__proto__')).toEqual({ type: 'md', line: 4 })
    expect(store.getPosition('constructor')).toEqual({ type: 'md', line: 9 })
    expect(store.getPosition('toString')).toBeNull()
  })

  it('handles special characters in filenames and paths', () => {
    const store = createProgressStore({ storage: createMemoryStorage(), now: createClock() })
    const name = 'Ünïcode & "quotes" #1 100%.zim'
    store.savePosition(name, { type: 'zim', path: 'A/C++_(language)?x=1#Hist%C3%B6ry' })
    expect(store.getPosition(name).path).toBe('A/C++_(language)?x=1#Hist%C3%B6ry')
  })

  it('rejects invalid positions and filenames', () => {
    const store = createProgressStore({ storage: createMemoryStorage(), now: createClock() })
    expect(store.savePosition('a.epub', { type: 'epub' })).toBe(false)
    expect(store.savePosition('a.epub', { type: 'nope', cfi: 'x' })).toBe(false)
    expect(store.savePosition('a.md', { type: 'md', line: Number.NaN })).toBe(false)
    expect(store.savePosition('', epub('x'))).toBe(false)
    expect(store.savePosition(null, epub('x'))).toBe(false)
    expect(store.getPosition('a.epub')).toBeNull()
  })
})

describe('progress store: eviction', () => {
  it('keeps only the most recent maxBooks books', () => {
    const storage = createMemoryStorage()
    const store = createProgressStore({ storage, maxBooks: 3, now: createClock() })
    for (const name of ['a', 'b', 'c', 'd', 'e']) store.touchBook(`${name}.epub`)
    const recent = store.listRecent(10).map((entry) => entry.filename)
    expect(recent).toEqual(['e.epub', 'd.epub', 'c.epub'])
    expect(store.getEntry('a.epub')).toBeNull()
    const raw = JSON.parse(storage.getItem(PROGRESS_STORAGE_KEY))
    expect(raw.books).toHaveLength(3)
  })

  it('re-opening an old book protects it from eviction', () => {
    const store = createProgressStore({ storage: createMemoryStorage(), maxBooks: 2, now: createClock() })
    store.touchBook('a.epub')
    store.touchBook('b.epub')
    store.touchBook('a.epub')
    store.touchBook('c.epub')
    expect(store.listRecent().map((entry) => entry.filename)).toEqual(['c.epub', 'a.epub'])
  })

  it('caps bookmarks per book, dropping the oldest', () => {
    const store = createProgressStore({ storage: createMemoryStorage(), maxBookmarks: 2, now: createClock() })
    store.addBookmark('a.md', { title: 'one', position: { type: 'md', line: 1 } })
    store.addBookmark('a.md', { title: 'two', position: { type: 'md', line: 2 } })
    store.addBookmark('a.md', { title: 'three', position: { type: 'md', line: 3 } })
    expect(store.getBookmarks('a.md').map((b) => b.title)).toEqual(['two', 'three'])
  })

  it('listRecent only includes opened books, newest first, honouring the limit', () => {
    const store = createProgressStore({ storage: createMemoryStorage(), now: createClock() })
    store.savePosition('never-opened.md', { type: 'md', line: 3 })
    store.touchBook('x.zim', { title: 'X', format: 'zim' })
    store.touchBook('y.zim')
    expect(store.listRecent(1).map((e) => e.filename)).toEqual(['y.zim'])
    expect(store.listRecent().map((e) => e.filename)).toEqual(['y.zim', 'x.zim'])
    expect(store.listRecent()[1].title).toBe('X')
  })
})

describe('progress store: resilience', () => {
  it('treats corrupt JSON as empty and recovers on next write', () => {
    const storage = createMemoryStorage({ [PROGRESS_STORAGE_KEY]: '{not json' })
    const store = createProgressStore({ storage, now: createClock() })
    expect(store.listRecent()).toEqual([])
    expect(store.getPosition('a.epub')).toBeNull()
    store.savePosition('a.epub', epub('cfi-1'))
    expect(JSON.parse(storage.getItem(PROGRESS_STORAGE_KEY)).books[0].position.cfi).toBe('cfi-1')
  })

  it('drops malformed entries but keeps valid ones', () => {
    const storage = createMemoryStorage({
      [PROGRESS_STORAGE_KEY]: JSON.stringify({
        v: 1,
        books: [
          null,
          42,
          { filename: '' },
          { filename: 'ok.zim', lastOpened: 5, position: { type: 'zim', path: 'A/Ok' }, bookmarks: [{ id: 'x' }, 'junk'] },
          { filename: 'ok.zim', lastOpened: 1 },
          { filename: 'bad-pos.epub', lastOpened: 3, position: { type: 'epub', cfi: 7 } }
        ]
      })
    })
    const store = createProgressStore({ storage })
    expect(store.getPosition('ok.zim')).toEqual({ type: 'zim', path: 'A/Ok' })
    expect(store.getBookmarks('ok.zim')).toEqual([])
    expect(store.getPosition('bad-pos.epub')).toBeNull()
    expect(store.listRecent().map((e) => e.filename)).toEqual(['ok.zim', 'bad-pos.epub'])
  })

  it('ignores a stored value of the wrong shape', () => {
    for (const raw of ['[]', '"text"', 'null', '{"books":{}}']) {
      const store = createProgressStore({ storage: createMemoryStorage({ [PROGRESS_STORAGE_KEY]: raw }) })
      expect(store.listRecent()).toEqual([])
    }
  })

  it('never throws when storage throws, and keeps working in memory', () => {
    const throwing = {
      getItem: () => {
        throw new Error('SecurityError')
      },
      setItem: () => {
        throw new Error('QuotaExceededError')
      },
      removeItem: () => {
        throw new Error('SecurityError')
      }
    }
    const store = createProgressStore({ storage: throwing, now: createClock() })
    expect(() => store.touchBook('a.epub')).not.toThrow()
    expect(store.savePosition('a.epub', epub('cfi-1'))).toBe(true)
    expect(store.getPosition('a.epub')?.cfi).toBe('cfi-1')
    expect(store.addBookmark('a.epub', { title: 'B', position: epub('cfi-2') })).not.toBeNull()
    expect(store.getBookmarks('a.epub')).toHaveLength(1)
    expect(store.listRecent()).toHaveLength(1)
  })

  it('works with no storage at all', () => {
    const store = createProgressStore({ storage: null, now: createClock() })
    store.savePosition('a.md', { type: 'md', line: 12 })
    expect(store.getPosition('a.md')).toEqual({ type: 'md', line: 12 })
  })

  it('retries with a trimmed state when the quota is exceeded', () => {
    const storage = createMemoryStorage()
    let failNext = 0
    const original = storage.setItem
    storage.setItem = (key, value) => {
      if (failNext > 0) {
        failNext -= 1
        throw new Error('QuotaExceededError')
      }
      original(key, value)
    }
    const store = createProgressStore({ storage, now: createClock() })
    for (const name of ['a', 'b', 'c', 'd']) store.touchBook(`${name}.md`)
    failNext = 1
    store.touchBook('e.md')
    const persisted = JSON.parse(storage.getItem(PROGRESS_STORAGE_KEY)).books.map((b) => b.filename)
    expect(persisted[0]).toBe('e.md')
    expect(persisted.length).toBeLessThan(5)
  })
})

describe('progress store: bookmarks', () => {
  it('adds, de-duplicates, lists and removes bookmarks', () => {
    const store = createProgressStore({ storage: createMemoryStorage(), now: createClock() })
    const first = store.addBookmark('a.epub', { title: 'Intro', position: epub('cfi-a', 'Chapter 1') })
    expect(first).toMatchObject({ title: 'Intro', position: { type: 'epub', cfi: 'cfi-a' } })
    expect(typeof first.id).toBe('string')

    const dup = store.addBookmark('a.epub', { title: 'Renamed', position: epub('cfi-a') })
    expect(dup.id).toBe(first.id)
    expect(store.getBookmarks('a.epub')).toHaveLength(1)
    expect(store.getBookmarks('a.epub')[0].title).toBe('Renamed')

    const second = store.addBookmark('a.epub', { position: epub('cfi-b', 'Chapter 2') })
    expect(second.title).toBe('Chapter 2')
    expect(store.getBookmarks('a.epub').map((b) => b.id)).toEqual([first.id, second.id])

    expect(store.removeBookmark('a.epub', first.id)).toBe(true)
    expect(store.removeBookmark('a.epub', first.id)).toBe(false)
    expect(store.removeBookmark('other.epub', second.id)).toBe(false)
    expect(store.getBookmarks('a.epub').map((b) => b.id)).toEqual([second.id])
  })

  it('rejects bookmarks without a usable position', () => {
    const store = createProgressStore({ storage: createMemoryStorage(), now: createClock() })
    expect(store.addBookmark('a.pdf', { title: 'x', position: null })).toBeNull()
    expect(store.addBookmark('a.pdf', { title: 'x', position: { type: 'pdf', page: 0 } })).toBeNull()
    expect(store.getBookmarks('a.pdf')).toEqual([])
  })

  it('clips over-long titles and forgets books', () => {
    const store = createProgressStore({ storage: createMemoryStorage(), now: createClock() })
    const bookmark = store.addBookmark('a.md', { title: 'x'.repeat(1000), position: { type: 'md', line: 2 } })
    expect(bookmark.title.length).toBe(200)
    expect(store.forgetBook('a.md')).toBe(true)
    expect(store.getBookmarks('a.md')).toEqual([])
  })
})

describe('normalizePosition / positionKey', () => {
  it('normalizes markdown lines and fractions', () => {
    expect(normalizePosition({ type: 'md', line: 3.7 })).toEqual({ type: 'md', line: 3 })
    expect(normalizePosition({ type: 'md', line: -5 })).toEqual({ type: 'md', line: 1 })
    expect(normalizePosition({ type: 'md', fraction: 2 })).toEqual({ type: 'md', fraction: 1 })
  })

  it('keeps a PDF page and the offset within it', () => {
    expect(normalizePosition({ type: 'pdf', page: 12.9, offset: 0.333333, label: 'Page 12' })).toEqual({
      type: 'pdf',
      page: 12,
      offset: 0.3333,
      label: 'Page 12'
    })
    expect(normalizePosition({ type: 'pdf', page: 3, offset: 7 })).toEqual({ type: 'pdf', page: 3, offset: 1 })
    expect(normalizePosition({ type: 'pdf', page: 3, offset: 'x' })).toEqual({ type: 'pdf', page: 3 })
    expect(normalizePosition({ type: 'pdf', page: 0 })).toBeNull()
  })

  it('round-trips a PDF position and bookmarks through the store', () => {
    const store = createProgressStore({ storage: null })
    store.savePosition('guide.pdf', { type: 'pdf', page: 40, offset: 0.5 })
    expect(store.getPosition('guide.pdf')).toEqual({ type: 'pdf', page: 40, offset: 0.5 })
    store.addBookmark('guide.pdf', { title: 'Page 40', position: { type: 'pdf', page: 40, offset: 0.2 } })
    store.addBookmark('guide.pdf', { title: 'Knots', position: { type: 'pdf', page: 40, offset: 0.7 } })
    const bookmarks = store.getBookmarks('guide.pdf')
    expect(bookmarks).toHaveLength(1)
    expect(bookmarks[0].title).toBe('Knots')
  })

  it('produces stable keys per position type', () => {
    expect(positionKey(epub('c'))).toBe('epub:c')
    expect(positionKey({ type: 'zim', path: 'A/B' })).toBe('zim:A/B')
    expect(positionKey({ type: 'md', line: 4 })).toBe('md:line:4')
    expect(positionKey({ type: 'pdf', page: 2 })).toBe('pdf:2')
    expect(positionKey(null)).toBe('')
  })
})

describe('progress store: export / import', () => {
  const clockAt = (start) => {
    let t = start
    return () => (t += 1)
  }

  it('round-trips everything into an empty store', () => {
    const source = createProgressStore({ storage: createMemoryStorage(), now: createClock() })
    source.touchBook('a.epub', { title: 'A', format: 'epub' })
    source.savePosition('a.epub', epub('epubcfi(/6/2)', 'Ch 1'))
    source.addBookmark('a.epub', { title: 'Mark', position: epub('epubcfi(/6/8)') })
    source.savePosition('b.pdf', { type: 'pdf', page: 7 })

    const exported = JSON.parse(JSON.stringify(source.exportData()))
    expect(exported.format).toBe('fyr-reading-progress')

    const target = createProgressStore({ storage: createMemoryStorage(), now: createClock() })
    expect(target.importData(exported)).toEqual({ booksAdded: 2, booksUpdated: 0, bookmarksAdded: 1 })
    expect(target.getEntry('a.epub')).toEqual(source.getEntry('a.epub'))
    expect(target.getPosition('b.pdf')).toEqual({ type: 'pdf', page: 7 })
  })

  it('merges: newer position wins, bookmarks are united without duplicates', () => {
    const laptop = createProgressStore({ storage: createMemoryStorage(), now: clockAt(1_000) })
    laptop.savePosition('a.epub', epub('epubcfi(/6/2)', 'old'))
    laptop.addBookmark('a.epub', { title: 'Shared', position: epub('epubcfi(/6/4)') })
    laptop.savePosition('c.md', { type: 'md', line: 40 })

    const phone = createProgressStore({ storage: createMemoryStorage(), now: clockAt(5_000) })
    phone.savePosition('a.epub', epub('epubcfi(/6/10)', 'new'))
    phone.addBookmark('a.epub', { title: 'Shared (phone)', position: epub('epubcfi(/6/4)') })
    phone.addBookmark('a.epub', { title: 'Phone only', position: epub('epubcfi(/6/12)') })
    phone.savePosition('c.md', { type: 'md', line: 1 })

    // Phone data is newer for a.epub; laptop keeps its own title for the shared mark.
    const summary = laptop.importData(phone.exportData())
    expect(summary.bookmarksAdded).toBe(1)
    expect(laptop.getPosition('a.epub').cfi).toBe('epubcfi(/6/10)')
    const marks = laptop.getBookmarks('a.epub')
    expect(marks.map((m) => m.title)).toEqual(['Shared', 'Phone only'])

    // Importing the older laptop data into the phone does not roll it back.
    const phoneBefore = phone.getPosition('a.epub')
    const older = createProgressStore({ storage: createMemoryStorage(), now: clockAt(10) })
    older.savePosition('a.epub', epub('epubcfi(/6/1)', 'ancient'))
    phone.importData(older.exportData())
    expect(phone.getPosition('a.epub')).toEqual(phoneBefore)

    // Importing the same data twice changes nothing.
    const again = laptop.importData(phone.exportData())
    expect(again.bookmarksAdded).toBe(0)
    expect(laptop.getBookmarks('a.epub')).toHaveLength(2)
  })

  it('rejects foreign files and sanitizes hostile entries', () => {
    const store = createProgressStore({ storage: createMemoryStorage(), now: createClock() })
    expect(() => store.importData({ books: [] })).toThrow(/Not a Fyr reading progress file/)
    expect(() => store.importData(null)).toThrow()
    const result = store.importData({
      format: 'fyr-reading-progress',
      v: 1,
      books: [
        { filename: '__proto__', position: { type: 'md', line: 3 } },
        { filename: 'x.epub', position: { type: 'epub', cfi: 'x'.repeat(10_000) }, bookmarks: [{ id: 'b', position: { type: 'evil' } }] },
        { filename: '', position: { type: 'pdf', page: 1 } },
        'nonsense'
      ]
    })
    expect(result.booksAdded).toBe(2)
    expect(store.getPosition('__proto__')).toEqual({ type: 'md', line: 3 })
    expect(store.getPosition('x.epub')).toBeNull()
    expect(store.getBookmarks('x.epub')).toEqual([])
    expect(Object.prototype.line).toBeUndefined()
  })
})
