<template>
  <div class="books-page">
    <div class="books-layout">
      <aside class="books-library">
        <header class="library-header">
          <h2>Library</h2>
          <p>{{ filteredBooks.length }} item(s)</p>
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

        <p v-if="booksLoading" class="status-card status-loading">Loading books...</p>
        <p v-else-if="booksError" class="status-card status-error">{{ booksError }}</p>

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

      <section class="reader-stage">
        <div v-if="selectedBook" class="reader-shell">
          <header class="reader-header">
            <div>
              <h3>{{ selectedBook.title || getDisplayName(selectedBook.filename) }}</h3>
              <p class="reader-subtitle">{{ selectedBook.filename }}</p>
            </div>
            <div class="reader-badges">
              <span class="badge badge-format">{{ activeFormat.toUpperCase() }}</span>
              <span class="badge" :class="readerStatusClass">{{ readerStatusLabel }}</span>
            </div>
          </header>

          <div v-if="readerError" class="status-card status-error">{{ readerError }}</div>

          <div class="reader-canvas">
            <div v-if="isEpubSelected && epubBook" id="book-viewer" class="epub-viewer"></div>

            <div v-else-if="isMarkdownSelected" class="markdown-reader">
              <article class="markdown-content" v-html="markdownHtml"></article>
            </div>

            <div v-else-if="isPdfSelected" class="pdf-reader">
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
              <div class="zim-tools">
                <div class="zim-tools-row">
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

              <div class="zim-content">
                <iframe
                  v-if="shouldUseNativeZimAdapter && zimNativeArticle?.content"
                  ref="zimNativeFrameRef"
                  class="zim-native-frame"
                  :srcdoc="zimNativeArticle.content"
                  :style="zimFrameStyle"
                  @load="onZimFrameLoad"
                  scrolling="no"
                  sandbox="allow-scripts allow-same-origin"
                  title="ZIM article"
                ></iframe>
                <p v-else-if="shouldUseNativeZimAdapter" class="reader-subtle">No native article content was returned for this archive.</p>
                <p v-else class="status-card status-error">{{ nativeZimUnavailableMessage }}</p>
              </div>

              <footer class="zim-meta">
                <p v-if="zimAdapter">Adapter: {{ zimAdapter.mode }}</p>
                <p v-if="zimMeta">Archive size: {{ formatBytes(zimMeta.size_bytes) }}</p>
                <p v-if="shouldUseNativeZimAdapter && zimNativeArticle?.title">Article: {{ zimNativeArticle.title }}</p>
              </footer>
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
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useUnifiedReader } from '../modules/reader/useUnifiedReader'
import { apiService } from '../services/api'

const books = ref([])
const booksLoading = ref(false)
const booksError = ref(null)
const selectedBook = ref(null)
const searchQuery = ref('')
const zimSearchQuery = ref('')
const zimSearchLoading = ref(false)
const zimSearchResults = ref([])
const zimSearchRan = ref(false)
const zimFrameHeight = ref(null)
const zimNativeFrameRef = ref(null)

let zimFrameObserver = null
let zimFrameClickHandler = null
let zimFrameResizeHandler = null
let zimFrameSettleTimer = null

const {
  activeFormat,
  status: unifiedReaderStatus,
  error: readerError,
  epubBook,
  markdownHtml,
  zimMeta,
  zimAdapter,
  zimNativeArticle,
  pdfUrl,
  hasExtension,
  decodePathDeep,
  selectBook: selectWithUnifiedReader,
  loadNativeZimArticle,
  resize: resizeUnifiedReader,
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

const nativeZimUnavailableMessage = computed(() => {
  if (activeFormat.value !== 'zim') return ''
  return 'Native ZIM parsing is unavailable for this archive with the current parser implementation.'
})

const zimFrameStyle = computed(() => {
  if (!zimFrameHeight.value) {
    return null
  }

  return {
    height: `${zimFrameHeight.value}px`
  }
})

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

  return `${normalizedPath}${resolved.search}`
}

const syncZimFrameHeight = () => {
  const frame = zimNativeFrameRef.value
  const doc = frame?.contentDocument
  if (!doc) {
    return
  }

  const body = doc.body
  const docEl = doc.documentElement
  if (body) {
    body.style.overflowY = 'hidden'
    body.style.overflowX = 'hidden'
  }
  if (docEl) {
    docEl.style.overflowY = 'hidden'
    docEl.style.overflowX = 'hidden'
  }

  const measured = Math.max(
    body?.scrollHeight || 0,
    body?.offsetHeight || 0,
    docEl?.scrollHeight || 0,
    docEl?.offsetHeight || 0,
    560
  )

  zimFrameHeight.value = Math.min(12000, Math.ceil(measured + 8))
}

const clearZimFrameHooks = () => {
  const frame = zimNativeFrameRef.value
  const doc = frame?.contentDocument
  const win = frame?.contentWindow

  if (doc && zimFrameClickHandler) {
    doc.removeEventListener('click', zimFrameClickHandler, true)
  }

  if (win && zimFrameResizeHandler) {
    win.removeEventListener('resize', zimFrameResizeHandler)
  }

  if (zimFrameObserver) {
    zimFrameObserver.disconnect()
    zimFrameObserver = null
  }

  if (zimFrameSettleTimer) {
    clearInterval(zimFrameSettleTimer)
    zimFrameSettleTimer = null
  }

  zimFrameClickHandler = null
  zimFrameResizeHandler = null
}

const onZimFrameLoad = () => {
  clearZimFrameHooks()

  const frame = zimNativeFrameRef.value
  const doc = frame?.contentDocument
  const win = frame?.contentWindow
  if (!doc || !win) {
    return
  }

  zimFrameClickHandler = async (event) => {
    if (event.defaultPrevented) {
      return
    }

    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
      return
    }

    let anchor = event.target
    while (anchor && anchor.tagName !== 'A') {
      anchor = anchor.parentElement
    }
    if (!anchor) {
      return
    }

    const rawHref = anchor.getAttribute('href')
    const articlePath = resolveNativeArticlePath(rawHref)
    if (!articlePath || !selectedBook.value?.filename) {
      return
    }

    event.preventDefault()
    event.stopPropagation()

    try {
      await loadNativeZimArticle(selectedBook.value.filename, articlePath, apiService)
    } catch (error) {
      readerError.value = apiService.handleError(error)
    }
  }

  doc.addEventListener('click', zimFrameClickHandler, true)

  zimFrameResizeHandler = () => {
    syncZimFrameHeight()
  }
  win.addEventListener('resize', zimFrameResizeHandler)

  zimFrameObserver = new MutationObserver(() => {
    syncZimFrameHeight()
  })
  zimFrameObserver.observe(doc.documentElement, {
    childList: true,
    subtree: true,
    attributes: true,
    characterData: true
  })

  syncZimFrameHeight()

  let ticks = 0
  zimFrameSettleTimer = setInterval(() => {
    syncZimFrameHeight()
    ticks += 1
    if (ticks >= 16) {
      clearInterval(zimFrameSettleTimer)
      zimFrameSettleTimer = null
    }
  }, 250)
}

const isCurrentSearchResult = (path) => {
  const current = normalizePathKey(zimNativeArticle.value?.path)
  if (!current) return false
  return normalizePathKey(path) === current
}

const handleZimMessage = async (event) => {
  if (event.origin !== window.location.origin) return

  if (event.data?.type === 'zim-height') {
    const next = Number(event.data?.height)
    if (!Number.isFinite(next) || next <= 0) return
    zimFrameHeight.value = Math.max(560, Math.min(12000, Math.ceil(next)))
    return
  }

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
  clearZimFrameHooks()
  selectedBook.value = book
  zimFrameHeight.value = null
  zimSearchQuery.value = ''
  zimSearchResults.value = []
  zimSearchRan.value = false
  await selectWithUnifiedReader(book, apiService)
  requestAnimationFrame(() => resizeUnifiedReader())
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
  await loadBooks()
})

onBeforeUnmount(() => {
  window.removeEventListener('message', handleZimMessage)
  clearZimFrameHooks()
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

  min-height: calc(100vh - 200px);
}

.books-layout {
  display: grid;
  grid-template-columns: minmax(270px, 320px) 1fr;
  gap: 1rem;
  min-height: calc(100vh - 260px);
}

.books-library,
.reader-stage {
  background: linear-gradient(180deg, var(--panel) 0%, var(--panel-ink) 100%);
  border: 1px solid var(--line);
  border-radius: 12px;
  box-shadow: 0 12px 36px rgba(0, 0, 0, 0.18);
}

.books-library {
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
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
  overflow-y: auto;
  max-height: calc(100vh - 340px);
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
  padding: 1rem;
}

.reader-shell {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.reader-header {
  display: flex;
  justify-content: space-between;
  gap: 0.75rem;
  align-items: flex-start;
  border-bottom: 1px solid var(--line);
  padding-bottom: 0.7rem;
}

.reader-header h3 {
  margin: 0;
  color: var(--text);
  font-size: 1.12rem;
}

.reader-subtitle {
  margin: 0.2rem 0 0;
  color: var(--muted);
  font-size: 0.8rem;
}

.reader-badges {
  display: flex;
  gap: 0.4rem;
  flex-wrap: wrap;
}

.badge {
  border-radius: 999px;
  border: 1px solid var(--line);
  font-size: 0.72rem;
  padding: 0.2rem 0.55rem;
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

.status-card {
  border-radius: 10px;
  padding: 0.65rem 0.75rem;
  font-size: 0.84rem;
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
  min-height: calc(100vh - 360px);
}

.epub-viewer,
.markdown-reader,
.pdf-frame {
  width: 100%;
  min-height: calc(100vh - 360px);
  border-radius: 10px;
  background: #ffffff;
}

.epub-viewer,
.markdown-reader,
.pdf-frame {
  border: 1px solid #ccd4db;
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

.zim-reader {
  display: flex;
  flex-direction: column;
  gap: 0.7rem;
}

.zim-tools {
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 0.7rem;
  background: #151b20;
}

.zim-tools-row {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 0.5rem;
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
  border: 1px solid var(--line);
  border-radius: 10px;
  overflow: visible;
  background: #0f1418;
}

.zim-native-frame {
  width: 100%;
  min-height: 0;
  height: 560px;
  border: none;
  display: block;
  overflow: hidden;
}

.zim-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 0.8rem;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: #151b20;
  padding: 0.55rem 0.7rem;
  color: var(--muted);
  font-size: 0.8rem;
}

.zim-meta p {
  margin: 0;
}

.reader-empty {
  min-height: calc(100vh - 360px);
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
  }

  .books-list {
    max-height: 240px;
  }

  .reader-canvas,
  .epub-viewer,
  .markdown-reader,
  .pdf-frame,
  .zim-native-frame,
  .reader-empty {
    min-height: 520px;
  }
}

@media (max-width: 640px) {
  .reader-header {
    flex-direction: column;
  }

  .zim-tools-row {
    grid-template-columns: 1fr;
  }
}
</style>
