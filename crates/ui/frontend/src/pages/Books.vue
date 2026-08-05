<template>
  <div class="books-page">
    <div class="books-layout" :class="{ 'library-collapsed': libraryCollapsed }">
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
            class="search-input"
          />
        </div>

        <p class="library-hint">Supported formats: .epub, .pdf, .mobi, .md, .zim</p>

        <p v-if="booksLoading" class="status-card status-loading" role="status" aria-live="polite" aria-busy="true">Loading books...</p>
        <p v-else-if="booksError" class="status-card status-error" role="alert" aria-live="assertive">{{ booksError }}</p>

        <div v-else-if="filteredBooks.length" class="books-list">
          <button
            v-for="book in filteredBooks"
            :key="book.filename"
            type="button"
            class="book-item"
            :class="{ active: selectedBook?.filename === book.filename }"
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
        <div v-if="selectedBook" class="reader-shell" :class="{ 'reader-shell-focused': readerFocusMode }">
          <header class="reader-toolbar">
            <div class="reader-toolbar-main">
              <button
                type="button"
                class="reader-back"
                aria-label="Back to library"
                title="Back to library"
                @click="returnToLibrary"
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

            <div v-if="hasExtension(selectedBook.filename, '.zim')" class="reader-toolbar-search">
              <input
                v-model="zimSearchQuery"
                type="text"
                class="zim-search-input"
                :placeholder="shouldUseNativeZimAdapter ? 'Search article title or path' : 'Native ZIM mode is required for search'"
                :disabled="!shouldUseNativeZimAdapter"
                @keydown.enter.prevent="runZimSearch"
              />
              <button
                type="button"
                class="zim-search-button"
                :disabled="!shouldUseNativeZimAdapter"
                @click="runZimSearch"
              >
                Search
              </button>
            </div>
          </header>

          <div v-if="readerError" class="status-card status-error" role="alert" aria-live="assertive">{{ readerError }}</div>

          <div class="reader-canvas">
            <div v-if="isEpubSelected && epubBook" id="book-viewer" class="reader-surface epub-viewer"></div>

            <div v-else-if="isMarkdownSelected" class="reader-surface markdown-reader">
              <article class="markdown-content" v-html="markdownHtml"></article>
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
              <div class="zim-search-feedback">
                <p v-if="zimSearchLoading" class="reader-subtle">Searching archive...</p>

                <div v-else-if="zimSearchResults.length" class="zim-search-results">
                  <button
                    v-for="result in zimSearchResults"
                    :key="result.path"
                    type="button"
                    class="zim-search-result"
                    :class="{ current: isCurrentSearchResult(result.path) }"
                    :disabled="isCurrentSearchResult(result.path)"
                    :title="result.path"
                    @click="openZimSearchResult(result.path)"
                  >
                    {{ result.title || result.path }}{{ isCurrentSearchResult(result.path) ? ' (current)' : '' }}
                  </button>
                </div>
                <p v-else-if="zimSearchRan" class="reader-subtle">No matching articles found.</p>
              </div>

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
                <p v-else class="status-card status-error">{{ nativeZimUnavailableMessage }}</p>
              </div>

            </div>

            <div v-else class="status-card status-warning">
              Select an EPUB, Markdown file, PDF, or ZIM in the library.
            </div>
          </div>
        </div>

        <div v-else class="reader-empty">
          <p>Pick a book from the library to open the reader.</p>
        </div>
      </section>
    </div>
  </div>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { useUnifiedReader } from '../modules/reader/useUnifiedReader'
import { apiService } from '../services/api'

const books = ref([])
const booksLoading = ref(false)
const booksError = ref(null)
const selectedBook = ref(null)
const libraryCollapsed = ref(false)
const readerFocusMode = ref(false)
const searchQuery = ref('')
const zimSearchQuery = ref('')
const zimSearchLoading = ref(false)
const zimSearchResults = ref([])
const zimSearchRan = ref(false)
const zimNativeFrameRef = ref(null)

const {
  activeFormat,
  status: unifiedReaderStatus,
  error: readerError,
  epubBook,
  markdownHtml,
  zimMeta,
  zimAdapter,
  zimNativeArticle,
  zimPendingHash,
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

const returnToLibrary = () => {
  readerFocusMode.value = false
  libraryCollapsed.value = false
  selectedBook.value = null
  zimSearchQuery.value = ''
  zimSearchResults.value = []
  zimSearchRan.value = false
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
      const target = doc.getElementById(targetId) || doc.querySelector(`[name="${targetId}"]`)
      if (target) {
        target.scrollIntoView()
      }
    } catch {
      // ignore scroll errors
    }
    zimPendingHash.value = null
  }
}

const isCurrentSearchResult = (path) => {
  const current = normalizePathKey(zimNativeArticle.value?.path)
  if (!current) return false
  return normalizePathKey(path) === current
}

const handleZimMessage = async (event) => {
  if (event.origin !== window.location.origin) return

  if (event.data?.type !== 'zim-navigate') return

  if (!selectedBook.value?.filename) return

  const articlePath = resolveNativeArticlePath(event.data.href)
  if (!articlePath) return

  try {
    await loadNativeZimArticle(selectedBook.value.filename, articlePath, apiService)
  } catch (error) {
    readerError.value = apiService.handleError(error)
  }
}

const runZimSearch = async () => {
  if (!selectedBook.value?.filename || !shouldUseNativeZimAdapter.value) {
    return
  }

  readerError.value = null
  const q = String(zimSearchQuery.value || '').trim()
  if (!q) {
    zimSearchRan.value = false
    zimSearchResults.value = []
    return
  }

  zimSearchLoading.value = true
  zimSearchRan.value = false
  try {
    const response = await apiService.getZimNativeSearch(selectedBook.value.filename, q, 24)
    if (typeof response === 'string') {
      throw new Error('Native search endpoint returned HTML. Restart the server to load the latest backend routes.')
    }
    zimSearchResults.value = Array.isArray(response?.results) ? response.results : []
    zimSearchRan.value = true
  } catch (error) {
    readerError.value = apiService.handleError(error)
    zimSearchResults.value = []
    zimSearchRan.value = true
  } finally {
    zimSearchLoading.value = false
  }
}

const openZimSearchResult = async (resultPath) => {
  if (!selectedBook.value?.filename || !resultPath) {
    return
  }

  if (isCurrentSearchResult(resultPath)) {
    return
  }

  try {
    await loadNativeZimArticle(selectedBook.value.filename, resultPath, apiService)
  } catch (error) {
    readerError.value = apiService.handleError(error)
  }
}

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

const selectBook = async (book) => {
  libraryCollapsed.value = true
  selectedBook.value = book
  zimSearchQuery.value = ''
  zimSearchResults.value = []
  zimSearchRan.value = false
  await nextTick()
  await selectWithUnifiedReader(book, apiService)
}

const handleReaderKeyboard = (event) => {
  if (event.key === 'Escape' && readerFocusMode.value) {
    setReaderFocus(false)
  }
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
  await loadBooks()
})

onBeforeUnmount(() => {
  window.removeEventListener('message', handleZimMessage)
  window.removeEventListener('keydown', handleReaderKeyboard)
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

.markdown-content {
  max-width: 920px;
  margin: 0 auto;
  padding: 1.5rem;
  color: #111111;
  line-height: 1.6;
}

.markdown-content :deep(pre) {
  background: #f4f6f8;
  border: 1px solid #dbe1e6;
  border-radius: 6px;
  padding: 0.85rem;
  overflow-x: auto;
}

.markdown-content :deep(code) {
  background: #eef1f4;
  border-radius: 4px;
  padding: 0.1rem 0.25rem;
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