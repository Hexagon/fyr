import { nextTick, ref, shallowRef } from 'vue'
import EPub from 'epubjs'
import { DEFAULT_READER_PREFS, READER_THEMES, READER_THEME_COLORS } from '../library/readerPrefs'

const safeDestroy = (obj) => {
  if (!obj) return
  try {
    obj.destroy()
  } catch {
    // Best-effort cleanup for epubjs internals.
  }
}

/** Longest we wait for epub.js work to settle before destroying anyway. */
const TEARDOWN_SETTLE_MS = 5000

/**
 * Destroy a rendition/book only after epub.js has finished the async work it
 * already started. Destroying synchronously while `display()` or the book's
 * background loaders (`book.loaded.*`) are still running makes epub.js read
 * `book.package` / `contents.themes` on destroyed objects and throw
 * ("reading 'package'", "reading 'replaceCss'") when the user leaves a book
 * right after opening it.
 */
export const destroyWhenSettled = (renditionObj, bookObj, pending = [], settleMs = TEARDOWN_SETTLE_MS) => {
  const work = [...pending]
  if (bookObj?.loaded && typeof bookObj.loaded === 'object') {
    work.push(...Object.values(bookObj.loaded))
  }
  // `opened` resolves only after epub.js finishes resource replacement
  // (`replaceCss`), which can run after `ready`.
  if (bookObj?.opened) work.push(bookObj.opened)
  const waits = work.filter((p) => p && typeof p.then === 'function').map((p) => Promise.resolve(p).catch(() => {}))
  const teardown = () => {
    safeDestroy(renditionObj)
    safeDestroy(bookObj)
  }
  if (!waits.length) {
    teardown()
    return Promise.resolve()
  }
  let timer = null
  const timeout = new Promise((resolve) => {
    timer = setTimeout(resolve, settleMs)
  })
  return Promise.race([Promise.all(waits), timeout]).then(() => {
    clearTimeout(timer)
    teardown()
  })
}

const themeRules = (name) => {
  const colors = READER_THEME_COLORS[name] || READER_THEME_COLORS.light
  return {
    html: { 'background-color': colors.background },
    body: { 'background-color': colors.background, color: colors.text },
    a: { color: colors.link }
  }
}

const stripHash = (href) => String(href || '').split('#')[0]

/** Find a human label (chapter title) for a spine href from the book's TOC. */
const chapterLabel = (bookObj, href) => {
  const target = stripHash(href)
  if (!target) return ''
  const toc = bookObj?.navigation?.toc
  if (!Array.isArray(toc)) return ''
  const stack = [...toc]
  while (stack.length) {
    const item = stack.shift()
    const itemHref = stripHash(item?.href)
    if (itemHref && (itemHref === target || target.endsWith(`/${itemHref}`) || itemHref.endsWith(`/${target}`))) {
      return String(item.label || '').trim()
    }
    if (Array.isArray(item?.subitems)) stack.push(...item.subitems)
  }
  return ''
}

export const useEpubReader = () => {
  const book = shallowRef(null)
  const rendition = shallowRef(null)
  /** Last position reported by epub.js `relocated` ({ type, cfi, href, label }). */
  const position = ref(null)
  let currentPrefs = { ...DEFAULT_READER_PREFS }
  /** In-flight `rendition.display()` of the current book, if any. */
  let pendingDisplay = null

  const dispose = () => {
    const renditionObj = rendition.value
    const bookObj = book.value
    const pending = pendingDisplay ? [pendingDisplay] : []
    rendition.value = null
    book.value = null
    position.value = null
    pendingDisplay = null
    destroyWhenSettled(renditionObj, bookObj, pending)
  }

  const applyPrefs = (prefs) => {
    currentPrefs = { ...currentPrefs, ...(prefs || {}) }
    const renditionObj = rendition.value
    if (!renditionObj) return
    try {
      const colors = READER_THEME_COLORS[currentPrefs.theme] || READER_THEME_COLORS.light
      renditionObj.themes.select(currentPrefs.theme)
      // Overrides use !important so publisher CSS cannot keep dark text on a dark theme.
      renditionObj.themes.override('color', colors.text, true)
      renditionObj.themes.override('background-color', colors.background, true)
      renditionObj.themes.fontSize(`${currentPrefs.fontScale}%`)
    } catch {
      // Theme application is cosmetic; never break reading over it.
    }
  }

  /**
   * Open an EPUB. `token.cancelled` is checked after every await so a slow
   * load never overwrites a book the user switched to in the meantime.
   */
  const open = async (descriptor, token = null, options = {}) => {
    const isStale = () => Boolean(token?.cancelled)
    const bookObj = new EPub(descriptor.content_url)

    try {
      await bookObj.ready
    } catch (error) {
      safeDestroy(bookObj)
      throw error
    }
    if (isStale()) {
      destroyWhenSettled(null, bookObj)
      return
    }

    // Mount #book-viewer (v-if on book) before epub.js looks it up.
    book.value = bookObj
    await nextTick()
    if (isStale()) return // dispose() of the newer selection already destroyed it

    // Pass the element itself: epub.js looks a string id up only later, from
    // a queue that waits for `book.opened`, by which time a quick "back"
    // may have unmounted it ("Not an Element").
    const viewerElement = document.getElementById('book-viewer')
    if (!viewerElement) return
    const renditionObj = bookObj.renderTo(viewerElement, {
      width: '100%',
      height: '100%',
      manager: 'continuous',
      flow: 'scrolled-doc',
      spread: 'none',
      allowScriptedContent: false
    })
    renditionObj.flow('scrolled-doc')
    renditionObj.spread('none')
    rendition.value = renditionObj

    for (const name of READER_THEMES) {
      renditionObj.themes.register(name, themeRules(name))
    }
    applyPrefs(options.prefs)

    renditionObj.on('relocated', (location) => {
      if (isStale() || rendition.value !== renditionObj) return
      const start = location?.start
      if (!start?.cfi) return
      const next = {
        type: 'epub',
        cfi: start.cfi,
        href: start.href || '',
        label: chapterLabel(bookObj, start.href)
      }
      position.value = next
      options.onRelocated?.(next)
    })

    const startCfi = typeof options.startCfi === 'string' && options.startCfi ? options.startCfi : undefined
    const display = (target) => {
      const promise = renditionObj.display(target)
      pendingDisplay = promise
      return promise
    }
    try {
      await display(startCfi)
    } catch (error) {
      if (isStale()) return
      if (!startCfi) throw error
      // Stored CFI no longer valid (book replaced/re-encoded): start from the beginning.
      await display()
    } finally {
      if (rendition.value === renditionObj) pendingDisplay = null
    }
  }

  /** Jump to a CFI or spine href (e.g. a /api/library search hit path). */
  const goTo = async (target) => {
    if (!rendition.value || !target) return false
    await rendition.value.display(target)
    return true
  }

  return {
    book,
    position,
    open,
    goTo,
    applyPrefs,
    dispose
  }
}
