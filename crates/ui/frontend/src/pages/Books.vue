<template>
  <div class="books-page">
    <div class="books-layout" :class="{ 'library-collapsed': libraryCollapsed }">
      <button
        v-if="libraryCollapsed"
        type="button"
        class="library-toggle library-expand"
        aria-label="Expand Library panel"
        title="Expand library panel"
        @click="toggleLibrary"
      >
        »
      </button>
      <aside v-if="!libraryCollapsed" class="books-library">
        <header class="library-header">
          <div class="library-header-copy">
            <h2>Library</h2>
            <p>{{ filteredBooks.length }} item(s)</p>
          </div>
          <button
            type="button"
            class="library-toggle"
            aria-label="Collapse Library panel"
            title="Collapse library panel"
            @click="toggleLibrary"
          >
            «
          </button>
        </header>

        <div class="library-search">
          <input
            v-model="searchQuery"
            type="text"
            placeholder="Search by title or filename"
            aria-label="Filter library by title or filename"
            class="search-input"
          />
        </div>

        <p class="library-hint">Supported formats: .epub, .pdf, .mobi, .md, .zim</p>

        <details class="progress-transfer">
          <summary>Move reading progress</summary>
          <p class="library-hint">
            Positions and bookmarks are stored in this browser. Export them to a file and import it on another
            device; imports merge (newest position wins, bookmarks are combined).
          </p>
          <div class="progress-transfer-actions">
            <button type="button" class="progress-transfer-btn" @click="exportProgress">Export</button>
            <label class="progress-transfer-btn">
              Import
              <input type="file" accept=".json,application/json" class="progress-file-input" @change="importProgress" />
            </label>
          </div>
          <p v-if="progressTransferStatus" class="library-hint" role="status">{{ progressTransferStatus }}</p>
        </details>

        <p v-if="booksLoading" class="status-card status-loading" role="status" aria-live="polite" aria-busy="true">Loading books...</p>
        <p v-else-if="booksError" class="status-card status-error" role="alert" aria-live="assertive">{{ booksError }}</p>

        <div v-else-if="filteredBooks.length" class="books-list">
          <button
            v-for="book in filteredBooks"
            :key="book.filename"
            type="button"
            class="book-item"
            :class="{ active: selectedBook?.filename === book.filename }"
            :aria-current="selectedBook?.filename === book.filename ? 'true' : undefined"
            @click="selectBook(book)"
          >
            <div class="book-title-row">
              <span class="book-title">{{ book.title || getDisplayName(book.filename) }}</span>
              <span class="book-format">{{ fileExt(book.filename) }}</span>
            </div>
            <span v-if="book.title" class="book-filename">{{ book.filename }}</span>
            <span class="book-size">{{ formatBytes(book.size) }}</span>
          </button>
        </div>

        <p v-else class="library-empty">
          No books found. <router-link to="/content">Add books</router-link>
        </p>
      </aside>

      <section class="reader-stage" :class="{ 'reader-stage-focused': readerFocusMode }">
        <p class="visually-hidden" aria-live="polite">{{ liveMessage }}</p>

        <div v-if="selectedBook" class="reader-shell" :class="{ 'reader-shell-focused': readerFocusMode }">
          <header class="reader-toolbar">
            <div class="reader-toolbar-main">
              <button
                type="button"
                class="reader-back"
                aria-label="Back to library"
                title="Back to library"
                @click="returnToLibrary()"
              >
                ←
              </button>
              <div class="reader-title-stack">
                <h3>{{ selectedBook.title || getDisplayName(selectedBook.filename) }}</h3>
                <p class="reader-subtitle">{{ selectedBook.filename }}</p>
              </div>
              <div class="reader-toolbar-meta">
                <span class="badge badge-format">{{ activeFormat.toUpperCase() }}</span>
                <span v-if="showReaderStatusBadge" class="badge" :class="readerStatusClass">{{ compactReaderStatusLabel }}</span>
                <span v-if="zimAdapter" class="badge">Adapter: {{ zimAdapter.mode }}</span>
                <span v-if="zimMeta" class="badge">Archive: {{ formatBytes(zimMeta.size_bytes) }}</span>
                <span v-if="shouldUseNativeZimAdapter && zimNativeArticle?.title" class="badge" :title="zimNativeArticle.title">
                  Article: {{ zimNativeArticle.title }}
                </span>
                <button
                  type="button"
                  class="reader-focus-toggle"
                  :aria-pressed="String(readerFocusMode)"
                  :title="readerFocusMode ? 'Exit focused reader mode (Esc)' : 'Enter focused reader mode'"
                  @click="toggleReaderFocus"
                >
                  {{ readerFocusMode ? 'Exit focus' : 'Focus mode' }}
                </button>
              </div>
            </div>

            <div class="reader-toolbar-tools" role="toolbar" aria-label="Reader tools">
              <div v-if="isZimSelected" class="toolbar-group" role="group" aria-label="Article history">
                <button
                  type="button"
                  class="toolbar-button"
                  aria-label="Previous article"
                  title="Previous article ([)"
                  :disabled="!zimCanBack || zimLoading"
                  @click="zimHistoryBack"
                >
                  ‹
                </button>
                <button
                  type="button"
                  class="toolbar-button"
                  aria-label="Next article"
                  title="Next article (])"
                  :disabled="!zimCanForward || zimLoading"
                  @click="zimHistoryForward"
                >
                  ›
                </button>
              </div>

              <div v-if="supportsDisplayPrefs" class="toolbar-group" role="group" aria-label="Display settings">
                <button
                  type="button"
                  class="toolbar-button"
                  aria-label="Smaller text"
                  title="Smaller text (-)"
                  :disabled="readerPrefs.fontScale <= FONT_SCALE_MIN"
                  @click="changeFontScale(-1)"
                >
                  A−
                </button>
                <span class="toolbar-value" aria-label="Text size">{{ readerPrefs.fontScale }}%</span>
                <button
                  type="button"
                  class="toolbar-button"
                  aria-label="Larger text"
                  title="Larger text (+)"
                  :disabled="readerPrefs.fontScale >= FONT_SCALE_MAX"
                  @click="changeFontScale(1)"
                >
                  A+
                </button>
                <select v-model="themeModel" class="toolbar-select" aria-label="Reader theme">
                  <option v-for="theme in READER_THEMES" :key="theme" :value="theme">{{ themeLabel(theme) }}</option>
                </select>
              </div>

              <div class="toolbar-group" role="group" aria-label="Bookmarks">
                <button
                  type="button"
                  class="toolbar-button"
                  aria-label="Add bookmark at the current position"
                  :title="supportsBookmarks ? 'Add bookmark (b)' : 'Bookmarks are not available for this format'"
                  :disabled="!canBookmarkNow"
                  @click="addBookmarkHere"
                >
                  ＋ Bookmark
                </button>
                <button
                  type="button"
                  class="toolbar-button"
                  :aria-expanded="bookmarksOpen ? 'true' : 'false'"
                  aria-controls="reader-bookmarks"
                  :aria-label="`Show bookmarks (${bookmarks.length})`"
                  @click="bookmarksOpen = !bookmarksOpen"
                >
                  Bookmarks ({{ bookmarks.length }})
                </button>
              </div>
            </div>

            <form
              v-if="hasExtension(selectedBook.filename, '.zim') || supportsBookSearch"
              class="reader-toolbar-search"
              role="search"
              @submit.prevent="runBookSearch"
            >
              <input
                ref="bookSearchInputRef"
                v-model="bookSearchQuery"
                type="text"
                class="zim-search-input"
                :placeholder="bookSearchPlaceholder"
                :disabled="!supportsBookSearch"
                aria-label="Search in this book"
              />
              <button
                type="submit"
                class="zim-search-button"
                :disabled="!supportsBookSearch"
              >
                Search
              </button>
            </form>
          </header>

          <div v-if="bookmarksOpen" id="reader-bookmarks" class="bookmarks-panel">
            <p v-if="!bookmarks.length" class="reader-subtle">
              No bookmarks yet. Use “＋ Bookmark” (or press b) to save the current position.
            </p>
            <ul v-else class="bookmark-list">
              <li v-for="bookmark in bookmarks" :key="bookmark.id" class="bookmark-item">
                <button
                  type="button"
                  class="bookmark-jump"
                  :aria-label="`Go to bookmark: ${bookmark.title}`"
                  @click="jumpToPosition(bookmark.position)"
                >
                  <span class="bookmark-title">{{ bookmark.title }}</span>
                  <span class="bookmark-date">{{ formatBookmarkDate(bookmark.createdAt) }}</span>
                </button>
                <button
                  type="button"
                  class="bookmark-remove"
                  :aria-label="`Remove bookmark: ${bookmark.title}`"
                  title="Remove bookmark"
                  @click="removeBookmark(bookmark)"
                >
                  ✕
                </button>
              </li>
            </ul>
          </div>

          <div v-if="readerNotice" class="status-card status-warning" role="status">{{ readerNotice }}</div>
          <div v-if="readerError" class="status-card status-error" role="alert" aria-live="assertive">{{ readerError }}</div>

          <div v-if="bookSearchLoading || bookSearchResults.length || bookSearchRan" class="zim-search-feedback">
            <p v-if="bookSearchLoading" class="reader-subtle">Searching...</p>

            <div v-else-if="bookSearchResults.length" class="zim-search-results">
              <button
                v-for="(result, index) in bookSearchResults"
                :key="`${index}:${result.path}`"
                type="button"
                class="zim-search-result"
                :class="{ current: isCurrentSearchResult(result.path), detailed: !isZimSelected && result.snippet }"
                :disabled="isCurrentSearchResult(result.path)"
                :title="result.snippet || result.path"
                @click="openSearchResult(result)"
              >
                <span class="result-title">{{ result.title || result.path }}{{ isCurrentSearchResult(result.path) ? ' (current)' : '' }}</span>
                <span v-if="!isZimSelected && result.snippet" class="result-snippet">{{ result.snippet }}</span>
              </button>
            </div>
            <p v-else-if="bookSearchRan" class="reader-subtle">No matches found.</p>
          </div>

          <div class="reader-canvas">
            <div
              v-if="isEpubSelected && epubBook"
              id="book-viewer"
              class="reader-surface epub-viewer"
              :style="{ background: themeColors.background }"
            ></div>

            <div
              v-else-if="isMarkdownSelected"
              ref="markdownSurfaceRef"
              class="reader-surface markdown-reader"
              :style="markdownStyle"
              @scroll.passive="onMarkdownScroll"
            >
              <article ref="markdownContentRef" class="markdown-content" v-html="markdownHtml"></article>
            </div>

            <div v-else-if="isPdfSelected" class="reader-surface pdf-reader">
              <iframe
                class="pdf-frame"
                :src="pdfUrl || selectedPdfUrl"
                title="PDF reader"
                loading="lazy"
              ></iframe>
              <p class="reader-subtle">
                If inline PDF rendering is unavailable,
                <a :href="pdfUrl || selectedPdfUrl" target="_blank" rel="noopener noreferrer">open it in a new tab</a>.
              </p>
            </div>

            <div v-else-if="hasExtension(selectedBook.filename, '.zim')" class="zim-reader">
              <div class="reader-surface zim-content">
                <iframe
                  v-if="shouldUseNativeZimAdapter && zimNativeArticle?.content"
                  ref="zimNativeFrameRef"
                  class="zim-native-frame"
                  :srcdoc="zimNativeArticle.content"
                  @load="onZimFrameLoad"
                  scrolling="auto"
                  sandbox="allow-scripts allow-same-origin"
                  title="ZIM article"
                ></iframe>
                <p v-else-if="shouldUseNativeZimAdapter" class="reader-subtle">No native article content was returned for this archive.</p>
                <p v-else-if="zimAdapter" class="status-card status-error">{{ nativeZimUnavailableMessage }}</p>
              </div>

            </div>

            <div v-else-if="unifiedReaderStatus !== 'idle' && !unifiedReaderStatus.startsWith('loading')" class="status-card status-warning">
              Select an EPUB, Markdown file, PDF, or ZIM in the library.
            </div>
          </div>
        </div>

        <div v-else class="reader-empty">
          <p v-if="readerNotice" class="status-card status-warning" role="status">{{ readerNotice }}</p>
          <p>Pick a book from the library to open the reader.</p>
        </div>
      </section>
    </div>
  </div>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useUnifiedReader } from '../modules/reader/useUnifiedReader'
import { createArticleHistory } from '../modules/reader/articleHistory'
import { findBlockForLine, headingLabelFor, topVisibleLine } from '../modules/reader/markdownPosition'
import { shortcutForEvent } from '../modules/reader/shortcuts'
import { progressStore } from '../modules/library/progress'
import { datedFilename, readFileText, saveText } from '../services/fileTransfer'
import { BOOKS_ROUTE_PATH, buildReaderQuery, parseReaderQuery, readerStateEquals } from '../modules/library/deepLink'
import {
  FONT_SCALE_MAX,
  FONT_SCALE_MIN,
  FONT_SCALE_STEP,
  READER_THEME_COLORS,
  READER_THEMES,
  loadReaderPrefs,
  saveReaderPrefs
} from '../modules/library/readerPrefs'
import { apiService } from '../services/api'

const route = useRoute()
const router = useRouter()

const books = ref([])
const booksLoading = ref(false)
const booksLoaded = ref(false)
const booksError = ref(null)
const selectedBook = ref(null)
const libraryCollapsed = ref(false)
const readerFocusMode = ref(false)
const searchQuery = ref('')

const bookSearchQuery = ref('')
const bookSearchLoading = ref(false)
const bookSearchResults = ref([])
const bookSearchRan = ref(false)
/** Query of the last search actually run (mirrored to `?q=`). */
const lastSearchRun = ref('')
const bookSearchInputRef = ref(null)

const zimNativeFrameRef = ref(null)
const zimLoading = ref(false)
const zimCanBack = ref(false)
const zimCanForward = ref(false)
/** Path (+ #hash) of the ZIM article currently shown (mirrored to `?path=`). */
const currentZimEntry = ref('')

const markdownSurfaceRef = ref(null)
const markdownContentRef = ref(null)
const readerNotice = ref('')
const liveMessage = ref('')
const readerPrefs = ref(loadReaderPrefs())
const bookmarks = ref([])
const bookmarksOpen = ref(false)

const zimHistory = createArticleHistory(100)

let zimLoadSeq = 0
let bookSearchSeq = 0
let pendingSave = null
let pendingSaveTimer = null
let markdownRestoring = false
let markdownScrollTimer = null
let liveMessageTimer = null
// Signatures of queries we pushed with router.replace, so the route watcher
// can tell our own URL syncs apart from external navigation.
const ownQuerySignatures = new Set()

const {
  activeFormat,
  status: unifiedReaderStatus,
  error: readerError,
  epubBook,
  epubPosition,
  epubGoTo,
  epubApplyPrefs,
  markdownHtml,
  zimMeta,
  zimAdapter,
  zimNativeArticle,
  zimPendingHash,
  zimStartPathFailed,
  pdfUrl,
  hasExtension,
  decodePathDeep,
  selectBook: selectWithUnifiedReader,
  loadNativeZimArticle,
  dispose: disposeUnifiedReader
} = useUnifiedReader()

const isEpubSelected = computed(() => activeFormat.value === 'epub' && epubBook.value)
const isMarkdownSelected = computed(() => activeFormat.value === 'md')
const isPdfSelected = computed(() => activeFormat.value === 'pdf')
const isZimSelected = computed(() => activeFormat.value === 'zim')

const filteredBooks = computed(() => {
  const query = searchQuery.value.toLowerCase().trim()
  if (!query) return books.value

  return books.value.filter((book) => {
    const nameMatch = getDisplayName(book.filename).toLowerCase().includes(query)
    const titleMatch = book.title ? book.title.toLowerCase().includes(query) : false
    return nameMatch || titleMatch
  })
})

const selectedPdfUrl = computed(() => {
  if (!hasExtension(selectedBook.value?.filename, '.pdf')) return ''
  return `/data/books/${encodeURIComponent(selectedBook.value.filename)}`
})

const shouldUseNativeZimAdapter = computed(() => {
  return activeFormat.value === 'zim' && zimAdapter.value?.supports_native_render === true
})

const supportsBookSearch = computed(() => {
  if (isZimSelected.value) return shouldUseNativeZimAdapter.value
  return Boolean(isEpubSelected.value) || isMarkdownSelected.value
})

const bookSearchPlaceholder = computed(() => {
  if (isZimSelected.value) {
    return shouldUseNativeZimAdapter.value ? 'Search article title or path' : 'Native ZIM mode is required for search'
  }
  return 'Search in this book'
})

const supportsDisplayPrefs = computed(() => Boolean(isEpubSelected.value) || isMarkdownSelected.value)

const supportsBookmarks = computed(() => {
  if (isZimSelected.value) return shouldUseNativeZimAdapter.value
  return Boolean(isEpubSelected.value) || isMarkdownSelected.value
})

const canBookmarkNow = computed(() => {
  if (!supportsBookmarks.value) return false
  if (isEpubSelected.value) return Boolean(epubPosition.value?.cfi)
  if (isZimSelected.value) return Boolean(currentZimEntry.value)
  return Boolean(markdownHtml.value)
})

const themeColors = computed(() => READER_THEME_COLORS[readerPrefs.value.theme] || READER_THEME_COLORS.light)

const markdownStyle = computed(() => ({
  '--reader-bg': themeColors.value.background,
  '--reader-text': themeColors.value.text,
  '--reader-link': themeColors.value.link,
  '--reader-code': themeColors.value.code,
  '--reader-border': themeColors.value.border,
  fontSize: `${readerPrefs.value.fontScale}%`
}))

const themeModel = computed({
  get: () => readerPrefs.value.theme,
  set: (theme) => updatePrefs({ theme })
})

const themeLabel = (theme) => ({ light: 'Light', sepia: 'Sepia', dark: 'Dark' })[theme] || theme

const readerStatusClass = computed(() => {
  if (readerError.value) return 'badge-error'
  if (unifiedReaderStatus.value.startsWith('loading')) return 'badge-loading'
  if (unifiedReaderStatus.value.startsWith('opened')) return 'badge-ready'
  if (unifiedReaderStatus.value === 'unsupported format') return 'badge-warning'
  return 'badge-idle'
})

const readerStatusLabel = computed(() => {
  if (unifiedReaderStatus.value === 'idle') return 'idle'
  return unifiedReaderStatus.value
})

const compactReaderStatusLabel = computed(() => {
  const label = readerStatusLabel.value
  if (!label || label === 'idle') return ''
  if (label.startsWith('opened ')) return ''
  if (label.startsWith('loading ')) return 'loading'
  return label
})

const showReaderStatusBadge = computed(() => compactReaderStatusLabel.value.length > 0)

const nativeZimUnavailableMessage = computed(() => {
  if (activeFormat.value !== 'zim') return ''
  return 'Native ZIM parsing is unavailable for this archive with the current parser implementation.'
})

const toggleLibrary = () => {
  libraryCollapsed.value = !libraryCollapsed.value
}

const setReaderFocus = (enabled) => {
  readerFocusMode.value = enabled
  if (enabled) {
    libraryCollapsed.value = true
  }
}

const toggleReaderFocus = () => {
  setReaderFocus(!readerFocusMode.value)
}

// ---------------------------------------------------------------------------
// Announcements (screen readers) and small helpers
// ---------------------------------------------------------------------------

const announce = (message) => {
  liveMessage.value = message
  if (liveMessageTimer) clearTimeout(liveMessageTimer)
  liveMessageTimer = setTimeout(() => {
    liveMessage.value = ''
    liveMessageTimer = null
  }, 4000)
}

const formatBookmarkDate = (timestamp) => {
  if (!timestamp) return ''
  try {
    return new Date(timestamp).toLocaleString()
  } catch {
    return ''
  }
}

// ---------------------------------------------------------------------------
// URL <-> reader state
// ---------------------------------------------------------------------------

const querySignature = (query) => JSON.stringify(buildReaderQuery(query))

const currentReaderState = () => ({
  file: selectedBook.value?.filename || null,
  path: isZimSelected.value ? currentZimEntry.value || null : null,
  q: lastSearchRun.value || null
})

const replaceQuery = (state) => {
  if (route.path !== BOOKS_ROUTE_PATH) return
  if (readerStateEquals(route.query, state)) return
  const query = buildReaderQuery(state)
  if (ownQuerySignatures.size > 20) ownQuerySignatures.clear()
  ownQuerySignatures.add(querySignature(query))
  router.replace({ path: BOOKS_ROUTE_PATH, query }).catch(() => {})
}

const syncUrl = () => replaceQuery(currentReaderState())

const applyRouteQuery = async (query) => {
  const parsed = parseReaderQuery(query)

  if (!parsed.file) {
    if (selectedBook.value) returnToLibrary({ updateUrl: false })
    return
  }

  const book = books.value.find((item) => item.filename === parsed.file)
  if (!book) {
    if (selectedBook.value) returnToLibrary({ updateUrl: false })
    readerNotice.value = `“${parsed.file}” is not in the library. It may have been removed or renamed.`
    replaceQuery({})
    return
  }

  if (selectedBook.value?.filename === book.filename) {
    if (parsed.path && isZimSelected.value && parsed.path !== currentZimEntry.value) {
      await loadZimArticle(parsed.path)
    }
    if (parsed.q && parsed.q !== lastSearchRun.value) {
      bookSearchQuery.value = parsed.q
      await runBookSearch()
    } else if (!parsed.q && lastSearchRun.value) {
      clearBookSearch()
    }
    return
  }

  await selectBook(book, { path: parsed.path, q: parsed.q })
}

watch(
  () => route.query,
  (query) => {
    if (route.path !== BOOKS_ROUTE_PATH || !booksLoaded.value) return
    const signature = querySignature(query)
    if (ownQuerySignatures.has(signature)) {
      // Our own sync landed; older pending signatures are obsolete.
      ownQuerySignatures.clear()
      return
    }
    if (readerStateEquals(query, currentReaderState())) return
    applyRouteQuery(query)
  }
)

// ---------------------------------------------------------------------------
// Reading position persistence
// ---------------------------------------------------------------------------

const flushPendingSave = () => {
  if (pendingSaveTimer) {
    clearTimeout(pendingSaveTimer)
    pendingSaveTimer = null
  }
  if (pendingSave) {
    progressStore.savePosition(pendingSave.filename, pendingSave.position)
    pendingSave = null
  }
}

const schedulePositionSave = (filename, position, delay = 800) => {
  if (!filename || !position) return
  if (pendingSave && pendingSave.filename !== filename) flushPendingSave()
  pendingSave = { filename, position }
  if (pendingSaveTimer) clearTimeout(pendingSaveTimer)
  pendingSaveTimer = setTimeout(flushPendingSave, delay)
}

// The Markdown pane scrolls inside its .reader-surface, so positions are
// measured relative to that element rather than the window.
const currentMarkdownPosition = () => {
  const container = markdownContentRef.value
  const surface = markdownSurfaceRef.value
  if (!container || !surface) return null
  const line = topVisibleLine(container, surface.getBoundingClientRect().top) || 1
  const block = findBlockForLine(container, line)
  const heading = headingLabelFor(container, block)
  return { type: 'md', line, label: heading || `Line ${line}` }
}

const currentPosition = () => {
  if (isEpubSelected.value) return epubPosition.value
  if (isZimSelected.value && currentZimEntry.value) {
    return { type: 'zim', path: currentZimEntry.value, label: zimNativeArticle.value?.title || currentZimEntry.value }
  }
  if (isMarkdownSelected.value) return currentMarkdownPosition()
  return null
}

const scrollToMarkdownLine = (line, { highlight = false } = {}) => {
  const surface = markdownSurfaceRef.value
  const block = findBlockForLine(markdownContentRef.value, line)
  if (!surface || !block) return false
  const offset = block.getBoundingClientRect().top - surface.getBoundingClientRect().top
  surface.scrollTop = Math.max(0, surface.scrollTop + offset - 12)
  if (highlight) {
    block.classList.add('md-block-target')
    setTimeout(() => block.classList.remove('md-block-target'), 1600)
  }
  return true
}

const onMarkdownScroll = () => {
  if (!isMarkdownSelected.value || markdownRestoring || !selectedBook.value) return
  if (markdownScrollTimer) return
  markdownScrollTimer = setTimeout(() => {
    markdownScrollTimer = null
    if (!isMarkdownSelected.value || markdownRestoring || !selectedBook.value) return
    const position = currentMarkdownPosition()
    if (position) schedulePositionSave(selectedBook.value.filename, position)
  }, 300)
}

const onPageHide = () => flushPendingSave()
const onVisibilityChange = () => {
  if (document.visibilityState === 'hidden') flushPendingSave()
}

// ---------------------------------------------------------------------------
// Display preferences
// ---------------------------------------------------------------------------

const updatePrefs = async (patch) => {
  // Keep the Markdown reading position stable across font-size changes.
  const anchor = isMarkdownSelected.value ? currentMarkdownPosition() : null
  readerPrefs.value = saveReaderPrefs({ ...readerPrefs.value, ...patch })
  epubApplyPrefs(readerPrefs.value)
  if (anchor && anchor.line > 1) {
    await nextTick()
    scrollToMarkdownLine(anchor.line)
  }
}

const changeFontScale = (direction) => {
  updatePrefs({ fontScale: readerPrefs.value.fontScale + direction * FONT_SCALE_STEP })
}

// ---------------------------------------------------------------------------
// Bookmarks
// ---------------------------------------------------------------------------

const refreshBookmarks = () => {
  bookmarks.value = selectedBook.value ? progressStore.getBookmarks(selectedBook.value.filename) : []
}

// Export/import of reading progress + bookmarks (client-side only).
const progressTransferStatus = ref('')

const exportProgress = () => {
  const data = progressStore.exportData()
  saveText(JSON.stringify(data, null, 2), datedFilename('reading-progress'))
  progressTransferStatus.value = `Exported ${data.books.length} book(s).`
}

const importProgress = async (event) => {
  const input = event.target
  const file = input.files?.[0]
  if (!file) return
  try {
    const raw = JSON.parse(await readFileText(file))
    const result = progressStore.importData(raw)
    refreshBookmarks()
    progressTransferStatus.value =
      `Imported: ${result.booksAdded} new book(s), ${result.booksUpdated} updated, ${result.bookmarksAdded} bookmark(s) added.`
  } catch (err) {
    progressTransferStatus.value = err instanceof SyntaxError ? 'Import failed: the file is not valid JSON.' : `Import failed: ${err?.message || err}`
  } finally {
    input.value = ''
  }
}

const addBookmarkHere = () => {
  if (!selectedBook.value || !canBookmarkNow.value) return
  const position = currentPosition()
  if (!position) return
  const before = bookmarks.value.length
  const title = position.label || (position.type === 'epub' ? 'EPUB location' : 'Bookmark')
  const saved = progressStore.addBookmark(selectedBook.value.filename, { title, position })
  refreshBookmarks()
  if (!saved) {
    announce('Could not save bookmark.')
  } else if (bookmarks.value.length === before) {
    announce(`Already bookmarked: ${saved.title}`)
  } else {
    announce(`Bookmark added: ${saved.title}`)
  }
}

const removeBookmark = (bookmark) => {
  if (!selectedBook.value) return
  progressStore.removeBookmark(selectedBook.value.filename, bookmark.id)
  refreshBookmarks()
  announce(`Bookmark removed: ${bookmark.title}`)
}

const jumpToPosition = async (position) => {
  if (!position || !selectedBook.value) return
  try {
    if (position.type === 'epub' && isEpubSelected.value) {
      await epubGoTo(position.cfi)
    } else if (position.type === 'zim' && isZimSelected.value) {
      await loadZimArticle(position.path)
    } else if (position.type === 'md' && isMarkdownSelected.value) {
      scrollToMarkdownLine(position.line || 1, { highlight: true })
    }
  } catch (error) {
    readerError.value = apiService.handleError(error)
  }
}

// ---------------------------------------------------------------------------
// ZIM article loading + in-reader history
// ---------------------------------------------------------------------------

const zimEntryOf = (article) => {
  const path = String(article?.path || '').replace(/^\/+/, '')
  if (!path) return ''
  return `${path}${article?.requested_hash || ''}`
}

const updateZimHistoryFlags = () => {
  zimCanBack.value = zimHistory.canBack()
  zimCanForward.value = zimHistory.canForward()
}

const onZimArticleShown = (article, { viaHistory = false } = {}) => {
  const entry = zimEntryOf(article)
  if (!entry || !selectedBook.value) return
  currentZimEntry.value = entry
  if (!viaHistory) zimHistory.visit(entry)
  updateZimHistoryFlags()
  progressStore.savePosition(selectedBook.value.filename, {
    type: 'zim',
    path: entry,
    label: article?.title || entry
  })
  syncUrl()
}

/** Load a ZIM article for the open book. Resolves to the article or null. */
const loadZimArticle = async (path, { viaHistory = false } = {}) => {
  const book = selectedBook.value
  if (!book || !path) return null
  const seq = ++zimLoadSeq
  zimLoading.value = true
  try {
    const article = await loadNativeZimArticle(book.filename, path, apiService)
    if (!article || selectedBook.value !== book) return null
    readerError.value = null
    onZimArticleShown(article, { viaHistory })
    return article
  } catch (error) {
    if (selectedBook.value === book) readerError.value = apiService.handleError(error)
    return null
  } finally {
    if (seq === zimLoadSeq) zimLoading.value = false
  }
}

const zimHistoryStep = async (direction) => {
  if (zimLoading.value) return
  const previousIndex = zimHistory.index
  const target = direction < 0 ? zimHistory.back() : zimHistory.forward()
  if (!target) return
  updateZimHistoryFlags()
  const article = await loadZimArticle(target, { viaHistory: true })
  if (!article) {
    zimHistory.restore(previousIndex)
    updateZimHistoryFlags()
  }
}

const zimHistoryBack = () => zimHistoryStep(-1)
const zimHistoryForward = () => zimHistoryStep(1)

const normalizePathKey = (value) => {
  return decodePathDeep(String(value || ''))
    .trim()
    .replace(/^\/+/, '')
    .toLowerCase()
}


const currentZimArticleBase = () => {
  const currentPath = String(zimNativeArticle.value?.path || '').trim()
  const normalized = `/${currentPath.replace(/^\/+/, '')}`
  const safe = normalized === '/' ? '/' : normalized
  return new URL(safe, window.location.origin)
}

const resolveNativeArticlePath = (rawHref) => {
  const href = String(rawHref || '').trim()
  if (!href || href.startsWith('#')) {
    return null
  }

  const lowerHref = href.toLowerCase()
  if (
    lowerHref.startsWith('mailto:') ||
    lowerHref.startsWith('javascript:') ||
    lowerHref.startsWith('data:') ||
    lowerHref.startsWith('vbscript:')
  ) {
    return null
  }

  if (!href.startsWith('/') && !/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(href)) {
    const hashIdx = href.indexOf('#')
    const searchIdx = href.indexOf('?')
    const firstFragment = hashIdx >= 0 && (searchIdx < 0 || hashIdx < searchIdx) ? hashIdx : searchIdx
    const pathPart = firstFragment >= 0 ? href.slice(0, firstFragment) : href
    const fragmentPart = firstFragment >= 0 ? href.slice(firstFragment) : ''
    const normalizedPath = decodePathDeep(pathPart).replace(/^\/+/, '')
    if (!normalizedPath) return null
    return `${normalizedPath}${fragmentPart}`
  }

  let resolved
  try {
    resolved = new URL(href, currentZimArticleBase())
  } catch {
    return null
  }

  if (resolved.origin !== window.location.origin) {
    return null
  }

  const normalizedPath = decodePathDeep(resolved.pathname).replace(/^\/+/, '')
  if (!normalizedPath) {
    return null
  }

  return `${normalizedPath}${resolved.search}${resolved.hash}`
}

const onZimFrameLoad = () => {
  const frame = zimNativeFrameRef.value
  const doc = frame?.contentDocument
  if (!doc) {
    return
  }

  // Scroll to hash fragment if one is pending
  const hash = zimPendingHash?.value
  if (hash) {
    try {
      const targetId = decodeURIComponent(hash.slice(1))
      const target = doc.getElementById(targetId) || doc.querySelector(`[name="${CSS.escape(targetId)}"]`)
      if (target) {
        target.scrollIntoView()
      }
    } catch {
      // ignore scroll errors
    }
    zimPendingHash.value = null
  }
}

const handleZimMessage = async (event) => {
  if (event.origin !== window.location.origin) return

  if (event.data?.type !== 'zim-navigate') return

  // Only the currently mounted article frame may navigate (not a frame of a
  // book the user just switched away from).
  const frameWindow = zimNativeFrameRef.value?.contentWindow
  if (!frameWindow || event.source !== frameWindow) return

  if (!selectedBook.value?.filename) return

  const articlePath = resolveNativeArticlePath(event.data.href)
  if (!articlePath) return

  await loadZimArticle(articlePath)
}

// ---------------------------------------------------------------------------
// In-book search (native ZIM search, unified library search for EPUB/MD)
// ---------------------------------------------------------------------------

const isCurrentSearchResult = (path) => {
  if (!isZimSelected.value) return false
  const current = normalizePathKey(zimNativeArticle.value?.path)
  if (!current) return false
  return normalizePathKey(path) === current
}

const clearBookSearch = () => {
  bookSearchSeq += 1
  bookSearchResults.value = []
  bookSearchRan.value = false
  bookSearchLoading.value = false
  lastSearchRun.value = ''
}

const runBookSearch = async () => {
  const book = selectedBook.value
  if (!book || !supportsBookSearch.value) {
    return
  }

  const q = String(bookSearchQuery.value || '').trim()
  if (!q) {
    clearBookSearch()
    syncUrl()
    return
  }

  const seq = ++bookSearchSeq
  readerError.value = null
  bookSearchLoading.value = true
  bookSearchRan.value = false
  lastSearchRun.value = q
  syncUrl()

  try {
    const response = isZimSelected.value
      ? await apiService.getZimNativeSearch(book.filename, q, 24)
      : await apiService.getLibraryBookSearch(book.filename, q, 24)
    if (seq !== bookSearchSeq || selectedBook.value !== book) return
    if (typeof response === 'string') {
      throw new Error('Search endpoint returned HTML. Restart the server to load the latest backend routes.')
    }
    bookSearchResults.value = Array.isArray(response?.results) ? response.results : []
    bookSearchRan.value = true
  } catch (error) {
    if (seq !== bookSearchSeq || selectedBook.value !== book) return
    readerError.value = apiService.handleError(error)
    bookSearchResults.value = []
    bookSearchRan.value = true
  } finally {
    if (seq === bookSearchSeq) bookSearchLoading.value = false
  }
}

const openSearchResult = async (result) => {
  const resultPath = result?.path
  if (!selectedBook.value || !resultPath) return

  if (isZimSelected.value) {
    if (isCurrentSearchResult(resultPath)) return
    await loadZimArticle(resultPath)
    return
  }

  if (isMarkdownSelected.value) {
    // Markdown hits are reported as "#<line>".
    const line = Number.parseInt(String(resultPath).replace(/^#/, ''), 10)
    if (Number.isFinite(line)) scrollToMarkdownLine(line, { highlight: true })
    return
  }

  if (isEpubSelected.value) {
    try {
      await epubGoTo(resultPath)
    } catch (error) {
      readerError.value = apiService.handleError(error)
    }
  }
}

// ---------------------------------------------------------------------------
// Keyboard: Esc leaves focus mode; reader shortcuts never fire while typing
// (see modules/reader/shortcuts.js)
// ---------------------------------------------------------------------------

const handleReaderKeyboard = (event) => {
  if (event.key === 'Escape' && readerFocusMode.value) {
    setReaderFocus(false)
    return
  }

  if (!selectedBook.value) return
  const action = shortcutForEvent(event)
  if (!action) return

  let handled = false
  if (action === 'history-back' && isZimSelected.value) {
    zimHistoryBack()
    handled = true
  } else if (action === 'history-forward' && isZimSelected.value) {
    zimHistoryForward()
    handled = true
  } else if (action === 'bookmark' && canBookmarkNow.value) {
    addBookmarkHere()
    handled = true
  } else if (action === 'focus-search' && supportsBookSearch.value && bookSearchInputRef.value) {
    bookSearchInputRef.value.focus()
    handled = true
  } else if (action === 'font-larger' && supportsDisplayPrefs.value) {
    changeFontScale(1)
    handled = true
  } else if (action === 'font-smaller' && supportsDisplayPrefs.value) {
    changeFontScale(-1)
    handled = true
  }

  if (handled) event.preventDefault()
}

// ---------------------------------------------------------------------------
// Book selection
// ---------------------------------------------------------------------------

const formatBytes = (bytes) => {
  if (!bytes) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  return `${Math.round((bytes / Math.pow(1024, i)) * 100) / 100} ${units[i]}`
}

const fileExt = (filename) => {
  const clean = String(filename || '')
  const idx = clean.lastIndexOf('.')
  return idx >= 0 ? clean.slice(idx + 1).toUpperCase() : 'FILE'
}

const getDisplayName = (filename) => String(filename || '').replace(/\.[^/.]+$/, '')

const resetReaderUi = () => {
  zimHistory.reset()
  updateZimHistoryFlags()
  currentZimEntry.value = ''
  zimLoadSeq += 1
  zimLoading.value = false
  bookSearchQuery.value = ''
  clearBookSearch()
  bookmarksOpen.value = false
  if (markdownScrollTimer) {
    clearTimeout(markdownScrollTimer)
    markdownScrollTimer = null
  }
}

/**
 * Open `book`. `options.path` (ZIM article) and `options.q` (in-book search)
 * come from deep links; otherwise the remembered position is restored.
 */
const selectBook = async (book, options = {}) => {
  flushPendingSave()
  resetReaderUi()
  readerNotice.value = ''
  libraryCollapsed.value = true
  selectedBook.value = book
  refreshBookmarks()
  progressStore.touchBook(book.filename, {
    title: book.title || getDisplayName(book.filename),
    format: fileExt(book.filename).toLowerCase()
  })
  bookSearchQuery.value = options.q || ''
  // Deep-link opens already have the right URL; plain clicks record the file now
  // so a reload during loading reopens the same book.
  if (!options.path && !options.q) syncUrl()

  const saved = progressStore.getPosition(book.filename)
  const deepLinkPath = options.path || null
  markdownRestoring = true

  await nextTick()
  if (selectedBook.value !== book) return

  const isCurrent = await selectWithUnifiedReader(book, apiService, {
    zimPath: deepLinkPath || (saved?.type === 'zim' ? saved.path : null),
    epubCfi: saved?.type === 'epub' ? saved.cfi : null,
    prefs: readerPrefs.value,
    onEpubRelocated: (position) => schedulePositionSave(book.filename, position)
  })
  if (!isCurrent || selectedBook.value !== book) return

  if (isZimSelected.value) {
    if (zimNativeArticle.value) onZimArticleShown(zimNativeArticle.value)
    if (zimStartPathFailed.value) {
      readerNotice.value = deepLinkPath
        ? `Article “${deepLinkPath}” was not found in this archive; showing the main page instead.`
        : 'The last read article is no longer available; showing the main page instead.'
    }
  }

  if (isMarkdownSelected.value) {
    await nextTick()
    if (selectedBook.value !== book) return
    if (saved?.type === 'md' && saved.line > 1) scrollToMarkdownLine(saved.line)
  }
  markdownRestoring = false

  if (options.q && supportsBookSearch.value) {
    await runBookSearch()
    if (selectedBook.value !== book) return
  }
  syncUrl()
}

const returnToLibrary = ({ updateUrl = true } = {}) => {
  flushPendingSave()
  resetReaderUi()
  disposeUnifiedReader()
  readerFocusMode.value = false
  libraryCollapsed.value = false
  selectedBook.value = null
  bookmarks.value = []
  markdownRestoring = false
  if (updateUrl) replaceQuery({})
}

const loadBooks = async () => {
  booksLoading.value = true
  booksError.value = null
  try {
    const response = await apiService.getBooks()
    books.value = response.data || []
  } catch (err) {
    booksError.value = apiService.handleError(err)
  } finally {
    booksLoading.value = false
  }
}

onMounted(async () => {
  window.addEventListener('message', handleZimMessage)
  window.addEventListener('keydown', handleReaderKeyboard)
  window.addEventListener('pagehide', onPageHide)
  document.addEventListener('visibilitychange', onVisibilityChange)
  await loadBooks()
  booksLoaded.value = true
  if (route.path === BOOKS_ROUTE_PATH) await applyRouteQuery(route.query)
})

onBeforeUnmount(() => {
  flushPendingSave()
  window.removeEventListener('message', handleZimMessage)
  window.removeEventListener('keydown', handleReaderKeyboard)
  window.removeEventListener('pagehide', onPageHide)
  document.removeEventListener('visibilitychange', onVisibilityChange)
  if (markdownScrollTimer) clearTimeout(markdownScrollTimer)
  if (liveMessageTimer) clearTimeout(liveMessageTimer)
  disposeUnifiedReader()
})
</script>

<style scoped>
.books-page {
  --panel: #1f2428;
  --panel-soft: #252d33;
  --panel-ink: #12161a;
  --line: #39434c;
  --text: #e7edf3;
  --muted: #a8b2bc;
  --brand: #0f766e;
  --brand-soft: #114d48;
  --error: #a2332f;
  --warning: #845c18;

  flex: 1;
  min-height: 0;
  overflow: hidden;
}

:global(.app-container:has(.books-page)) {
  height: 100dvh;
  min-height: 0;
  overflow: hidden;
}

:global(.app-container:has(.books-page) .navbar),
:global(.app-container:has(.books-page) .app-footer) {
  flex-shrink: 0;
}

:global(.page-content:has(.books-page)) {
  min-height: 0;
  display: flex;
  overflow: hidden;
  padding: 0.65rem 0.9rem 0.8rem;
}

:global(.app-container:has(.books-page) .app-footer) {
  display: none;
}

.books-layout {
  display: grid;
  grid-template-columns: minmax(270px, 320px) 1fr;
  gap: 1rem;
  height: 100%;
  min-height: 0;
  position: relative;
}

.books-layout.library-collapsed {
  grid-template-columns: 1fr;
  grid-template-rows: minmax(0, 1fr);
}

.books-library,
.reader-stage {
  background: linear-gradient(180deg, var(--panel) 0%, var(--panel-ink) 100%);
  border: 1px solid var(--line);
  border-radius: 12px;
  box-shadow: 0 12px 36px rgba(0, 0, 0, 0.18);
  min-height: 0;
}

.books-library {
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  overflow: hidden;
}

.books-layout.library-collapsed .reader-stage {
  min-height: 0;
}

/* The floating "expand library" button sits at the stage's top-left corner;
   keep the reader toolbar (Back button) out from under it. */
.books-layout.library-collapsed .reader-toolbar-main {
  padding-left: 2.6rem;
}

.library-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 0.6rem;
}

.library-header-copy {
  min-width: 0;
}

.library-header h2 {
  margin: 0;
  color: var(--text);
  font-size: 1.15rem;
}

.library-header p {
  margin: 0.2rem 0 0;
  color: var(--muted);
  font-size: 0.82rem;
}

.library-toggle {
  width: 2.2rem;
  height: 2.2rem;
  border: 1px solid #4f5d68;
  border-radius: 8px;
  background: linear-gradient(180deg, #11161a 0%, #0d1216 100%);
  color: var(--text);
  cursor: pointer;
  font-size: 1rem;
  font-weight: 700;
  line-height: 1;
  transition: transform 0.15s ease, border-color 0.15s ease, background 0.15s ease;
}

.library-expand {
  position: absolute;
  top: 1rem;
  left: 1rem;
  z-index: 10;
  flex-shrink: 0;
}

.library-toggle:hover {
  border-color: #40c0b5;
  background: #13353b;
  transform: translateY(-1px);
}

.library-toggle:focus-visible {
  outline: 2px solid #40c0b5;
  outline-offset: 2px;
}

.library-search {
  margin-top: 0.2rem;
}

.search-input,
.zim-search-input {
  width: 100%;
  background: #11161a;
  border: 1px solid var(--line);
  border-radius: 8px;
  color: var(--text);
  padding: 0.6rem 0.75rem;
}

.search-input:focus,
.zim-search-input:focus {
  outline: none;
  border-color: #1f9288;
  box-shadow: 0 0 0 3px rgba(31, 146, 136, 0.2);
}

.library-hint,
.reader-subtle {
  margin: 0;
  color: var(--muted);
  font-size: 0.8rem;
}

.progress-transfer {
  display: grid;
  gap: 0.4rem;
  font-size: 0.85rem;
}

.progress-transfer summary {
  cursor: pointer;
  color: var(--muted);
}

.progress-transfer-actions {
  display: flex;
  gap: 0.5rem;
  margin: 0.4rem 0;
}

.progress-transfer-btn {
  position: relative;
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 8px;
  background: transparent;
  color: inherit;
  padding: 0.35rem 0.75rem;
  cursor: pointer;
  font: inherit;
}

.progress-file-input {
  position: absolute;
  width: 1px;
  height: 1px;
  opacity: 0;
  overflow: hidden;
}

.books-list {
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.book-item {
  border: 1px solid var(--line);
  background: var(--panel-soft);
  border-radius: 10px;
  padding: 0.65rem 0.7rem;
  color: var(--text);
  text-align: left;
  cursor: pointer;
  transition: border-color 0.2s ease, transform 0.2s ease;
}

.book-item:hover {
  border-color: #1f9288;
  transform: translateY(-1px);
}

.book-item.active {
  border-color: #40c0b5;
  background: #1a3f44;
}

.book-title-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
}

.book-title {
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-weight: 600;
}

.book-format {
  background: rgba(64, 192, 181, 0.2);
  border: 1px solid rgba(64, 192, 181, 0.5);
  color: #b9fff6;
  border-radius: 999px;
  padding: 0.05rem 0.4rem;
  font-size: 0.68rem;
}

.book-filename,
.book-size {
  display: block;
  color: var(--muted);
  font-size: 0.75rem;
}

.reader-stage {
  padding: 0.55rem;
  display: flex;
  overflow: hidden;
}

.reader-shell {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.reader-toolbar {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  padding: 0.1rem 0 0.2rem;
}

.reader-toolbar-main {
  display: flex;
  align-items: flex-start;
  gap: 0.5rem;
  min-width: 0;
}

.reader-back {
  width: 2rem;
  height: 2rem;
  border: 1px solid #4f5d68;
  border-radius: 8px;
  background: #141a1f;
  color: var(--text);
  font-size: 1rem;
  line-height: 1;
  cursor: pointer;
  flex-shrink: 0;
}

.reader-back:hover {
  border-color: #40c0b5;
  background: #173a40;
}

.reader-back:focus-visible {
  outline: 2px solid #40c0b5;
  outline-offset: 2px;
}

.reader-title-stack {
  min-width: 0;
  flex: 1;
}

.reader-title-stack h3 {
  margin: 0;
  color: var(--text);
  font-size: 0.98rem;
  line-height: 1.25;
}

.reader-subtitle {
  margin: 0.08rem 0 0;
  color: var(--muted);
  font-size: 0.72rem;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.reader-toolbar-meta {
  display: flex;
  gap: 0.35rem;
  flex-wrap: wrap;
  justify-content: flex-end;
  min-width: 0;
}

.reader-focus-toggle {
  border-radius: 999px;
  border: 1px solid #2f8277;
  background: #16534c;
  color: #d8fff9;
  font-size: 0.7rem;
  padding: 0.12rem 0.55rem;
  cursor: pointer;
}

.reader-focus-toggle:hover {
  background: #1f6a62;
}

.reader-focus-toggle:focus-visible {
  outline: 2px solid #40c0b5;
  outline-offset: 2px;
}

.badge {
  border-radius: 999px;
  border: 1px solid var(--line);
  font-size: 0.7rem;
  padding: 0.12rem 0.45rem;
  color: var(--text);
  background: #172026;
}

.badge-format {
  border-color: rgba(64, 192, 181, 0.5);
  color: #b9fff6;
  background: rgba(64, 192, 181, 0.2);
}

.badge-idle { background: #222c35; }
.badge-loading { background: #21344a; border-color: #3b5c7e; }
.badge-ready { background: #1f4b45; border-color: #2f8277; }
.badge-error { background: #5d2727; border-color: #8f3434; }
.badge-warning { background: #4e3a1f; border-color: #7d5d2f; }

.reader-toolbar-search {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 0.5rem;
}

.status-card {
  border-radius: 10px;
  padding: 0.42rem 0.58rem;
  font-size: 0.8rem;
  margin: 0;
}

.status-loading {
  background: rgba(59, 92, 126, 0.3);
  border: 1px solid #3b5c7e;
  color: #d7e9ff;
}

.status-error {
  background: rgba(162, 51, 47, 0.25);
  border: 1px solid #a2332f;
  color: #ffe0df;
}

.status-warning {
  background: rgba(132, 92, 24, 0.25);
  border: 1px solid #845c18;
  color: #ffe8be;
}

.reader-canvas {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  display: flex;
}

.reader-stage-focused {
  padding: 0.35rem;
}

.reader-shell-focused .reader-toolbar {
  background: rgba(0, 0, 0, 0.22);
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 0.45rem;
}

.reader-canvas > * {
  min-width: 0;
}

.reader-surface {
  width: 100%;
  height: 100%;
  min-height: 0;
  border-radius: 10px;
  border: 1px solid #ccd4db;
  background: #ffffff;
  overflow: auto;
}

.epub-viewer {
  overflow: hidden;
}

.markdown-reader {
  background: var(--reader-bg, #ffffff);
}

.markdown-content {
  max-width: 920px;
  margin: 0 auto;
  padding: 1.5rem;
  color: var(--reader-text, #111111);
  line-height: 1.6;
}

.markdown-content :deep(a) {
  color: var(--reader-link, #1a5fb4);
}

.markdown-content :deep(pre) {
  background: var(--reader-code, #f4f6f8);
  border: 1px solid var(--reader-border, #dbe1e6);
  border-radius: 6px;
  padding: 0.85rem;
  overflow-x: auto;
}

.markdown-content :deep(code) {
  background: var(--reader-code, #eef1f4);
  border-radius: 4px;
  padding: 0.1rem 0.25rem;
}

.markdown-content :deep(.md-block) {
  transition: background-color 0.4s ease;
  border-radius: 4px;
}

.markdown-content :deep(.md-block-target) {
  background-color: rgba(64, 192, 181, 0.22);
}

.pdf-reader {
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
}

.pdf-frame {
  width: 100%;
  flex: 1;
  min-height: 0;
  border: none;
}

.zim-reader {
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
  flex: 1;
  width: 100%;
  height: 100%;
  min-height: 0;
}

.zim-search-feedback {
  min-height: 0;
}

.zim-search-button {
  border: 1px solid #2f8277;
  background: #16534c;
  color: #d8fff9;
  border-radius: 8px;
  padding: 0.45rem 0.75rem;
  cursor: pointer;
}

.zim-search-button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.zim-search-results {
  display: flex;
  flex-wrap: wrap;
  gap: 0.45rem;
  margin-top: 0.55rem;
}

.zim-search-result {
  border: 1px solid #3f4f74;
  background: #1d2740;
  color: #e8efff;
  border-radius: 999px;
  padding: 0.35rem 0.6rem;
  font-size: 0.78rem;
  cursor: pointer;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.zim-search-result.current,
.zim-search-result:disabled {
  background: #32363f;
  border-color: #505867;
  color: #b6bfcc;
  cursor: default;
}

.zim-content {
  border-color: var(--line);
  flex: 1;
  min-height: 0;
  display: flex;
  overflow: hidden;
  background: #0f1418;
}

.zim-native-frame {
  width: 100%;
  flex: 1;
  height: 100%;
  border: none;
  display: block;
  background: #ffffff;
}


.reader-toolbar-tools {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem 0.9rem;
  align-items: center;
}

.toolbar-group {
  display: flex;
  gap: 0.3rem;
  align-items: center;
}

.toolbar-button,
.toolbar-select {
  border: 1px solid #4f5d68;
  background: #141a1f;
  color: var(--text);
  border-radius: 8px;
  padding: 0.22rem 0.55rem;
  font-size: 0.76rem;
  cursor: pointer;
}

.toolbar-button:hover:not(:disabled) {
  border-color: #40c0b5;
  background: #173a40;
}

.toolbar-button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.toolbar-button:focus-visible,
.toolbar-select:focus-visible,
.bookmark-jump:focus-visible,
.bookmark-remove:focus-visible,
.zim-search-result:focus-visible,
.book-item:focus-visible {
  outline: 2px solid #40c0b5;
  outline-offset: 2px;
}

.toolbar-value {
  color: var(--muted);
  font-size: 0.74rem;
  min-width: 2.8rem;
  text-align: center;
}

.bookmarks-panel {
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 0.5rem 0.6rem;
  background: #151b20;
  flex-shrink: 0;
}

.bookmark-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
  max-height: 200px;
  overflow-y: auto;
}

.bookmark-item {
  display: flex;
  gap: 0.3rem;
}

.bookmark-jump {
  flex: 1;
  min-width: 0;
  display: flex;
  justify-content: space-between;
  gap: 0.6rem;
  text-align: left;
  border: 1px solid var(--line);
  background: var(--panel-soft);
  color: var(--text);
  border-radius: 8px;
  padding: 0.35rem 0.55rem;
  cursor: pointer;
}

.bookmark-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.bookmark-date {
  color: var(--muted);
  font-size: 0.72rem;
  white-space: nowrap;
}

.bookmark-remove {
  border: 1px solid var(--line);
  background: #2a1d1d;
  color: #ffd6d4;
  border-radius: 8px;
  padding: 0 0.6rem;
  cursor: pointer;
}

.zim-search-feedback {
  flex-shrink: 0;
  max-height: 30%;
  overflow-y: auto;
}

.zim-search-result .result-title {
  overflow: hidden;
  text-overflow: ellipsis;
}

.zim-search-result.detailed {
  border-radius: 8px;
  white-space: normal;
  display: flex;
  flex-direction: column;
  text-align: left;
  width: 100%;
}

.result-snippet {
  color: #b9c4dc;
  font-size: 0.72rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}

.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}

.reader-empty {
  height: 100%;
  width: 100%;
  display: grid;
  place-content: center;
  color: var(--muted);
  text-align: center;
}

.library-empty {
  color: var(--muted);
  font-style: italic;
}

.library-empty a,
.reader-subtle a {
  color: #7ee4da;
}

@media (max-width: 1080px) {
  .books-layout {
    grid-template-columns: 1fr;
    grid-template-rows: auto minmax(0, 1fr);
  }

  .books-layout.library-collapsed {
    grid-template-columns: 1fr;
    grid-template-rows: minmax(0, 1fr);
  }

  .books-list {
    max-height: 240px;
    flex: initial;
  }
}

@media (max-width: 640px) {
  :global(.page-content:has(.books-page)) {
    padding: 0.45rem 0.55rem 0.7rem;
  }

  .books-library,
  .reader-stage {
    border-radius: 10px;
  }

  .library-toggle {
    width: 2.4rem;
    height: 2.4rem;
  }

  .books-layout.library-collapsed .reader-toolbar-main {
    padding-left: 2.8rem;
  }

  .reader-toolbar-main {
    flex-wrap: wrap;
  }

  .reader-toolbar-meta {
    justify-content: flex-start;
  }

  .reader-subtitle {
    display: none;
  }
}
</style>