<template>
  <div class="books-page">
    <div class="books-container">
      <aside class="books-sidebar" :class="{ collapsed: sidebarCollapsed }">
        <div class="sidebar-header">
          <h3 v-if="!sidebarCollapsed">Books ({{ filteredBooks.length }})</h3>
          <button
            class="collapse-btn"
            @click="toggleSidebar"
            :title="sidebarCollapsed ? 'Expand book list' : 'Collapse book list'"
          >
            {{ sidebarCollapsed ? '»' : '«' }}
          </button>
        </div>

        <div v-if="!sidebarCollapsed" class="search-box">
          <input
            v-model="searchQuery"
            type="text"
            placeholder="Search books..."
            class="search-input"
          />
        </div>

        <p v-if="!sidebarCollapsed" class="hint-text">Supported book formats: .epub, .pdf, .mobi, .md, .zim</p>

        <p v-if="!sidebarCollapsed && booksLoading" class="status-state">Loading books...</p>
        <p v-else-if="!sidebarCollapsed && booksError" class="error-state">{{ booksError }}</p>
        <div v-else-if="!sidebarCollapsed && filteredBooks.length" class="books-list">
          <button
            v-for="book in filteredBooks"
            :key="book.filename"
            @click="selectBook(book)"
            class="book-item"
            :class="{ active: selectedBook?.filename === book.filename }"
          >
            <span class="book-icon">📖</span>
            <div class="book-details">
              <span class="book-name">{{ getDisplayName(book.filename) }}</span>
              <span class="book-size">{{ formatBytes(book.size) }}</span>
            </div>
          </button>
        </div>

        <p v-else-if="!sidebarCollapsed" class="empty-state">
          No books found. <router-link to="/content">Add books</router-link>
        </p>
      </aside>

      <div class="book-viewer">
        <div v-if="selectedBook" class="book-content">
          <div v-if="isEpubSelected && epubBook" id="book-viewer" class="book-reader"></div>
          <div v-else-if="isMarkdownSelected" class="markdown-reader">
            <article class="markdown-content" v-html="markdownHtml"></article>
          </div>
          <div v-else-if="isPdfSelected" class="pdf-reader">
            <iframe
              class="pdf-reader-frame"
              :src="selectedPdfUrl"
              title="PDF reader"
              loading="lazy"
            ></iframe>
            <p class="hint-text pdf-hint">
              If inline PDF rendering is unavailable in this browser,
              <a :href="selectedPdfUrl" target="_blank" rel="noopener noreferrer">open it in a new tab</a>.
            </p>
          </div>
          <div v-else-if="hasExtension(selectedBook.filename, '.zim')" class="zim-reader">
            <div class="zim-search">
              <div class="zim-search-row">
                <input
                  v-model="zimSearchQuery"
                  class="zim-search-input"
                  type="text"
                  :placeholder="shouldUseNativeZimAdapter ? 'Search article title or path' : 'Enable native ZIM mode to search'"
                  :disabled="!shouldUseNativeZimAdapter"
                  @keydown.enter.prevent="runZimSearch"
                />
                <button class="zim-search-btn" type="button" :disabled="!shouldUseNativeZimAdapter" @click="runZimSearch">Search</button>
              </div>
              <p v-if="zimSearchLoading" class="hint-text">Searching...</p>
              <div v-else-if="zimSearchResults.length" class="zim-search-results">
                <button
                  v-for="result in zimSearchResults"
                  :key="result.path"
                  class="zim-search-result"
                  :class="{ 'is-current': isCurrentSearchResult(result.path) }"
                  type="button"
                  :disabled="isCurrentSearchResult(result.path)"
                  @click="openZimSearchResult(result.path)"
                  :title="result.path"
                >
                  {{ result.title || result.path }}{{ isCurrentSearchResult(result.path) ? ' (current)' : '' }}
                </button>
              </div>
              <p v-else-if="zimSearchRan" class="hint-text">No matching articles found.</p>
            </div>
            <div v-if="shouldUseNativeZimAdapter" class="zim-native-panel">
              <div
                v-if="zimNativeArticle?.content"
                class="zim-native-article"
                @click="onNativeZimArticleClick"
                v-html="zimNativeArticle.content"
              ></div>
              <p v-else class="hint-text">No native article content was returned for this archive.</p>
            </div>
            <div class="zim-status-bar">
              <p v-if="unifiedReaderStatus !== 'idle'" class="hint-text">Reader module: {{ unifiedReaderStatus }}</p>
              <p v-if="zimAdapter" class="hint-text">Adapter mode: {{ zimAdapter.mode }}</p>
              <p v-if="zimMeta" class="hint-text">Archive size: {{ formatBytes(zimMeta.size_bytes) }}</p>
              <p v-if="shouldUseNativeZimAdapter" class="status-state">Native ZIM adapter path selected for this archive.</p>
              <p v-if="shouldUseNativeZimAdapter && zimNativeArticle?.title" class="hint-text">Article: {{ zimNativeArticle.title }}</p>
              <p v-if="!shouldUseNativeZimAdapter" class="error-state">{{ nativeZimUnavailableMessage }}</p>
              <p v-if="readerError" class="error-state">{{ readerError }}</p>
            </div>
          </div>
          <div v-else class="book-info-empty">
            Select an EPUB, Markdown file, PDF, or ZIM in the list to read it here.
          </div>
        </div>

        <div v-else class="empty-view">
          <p>Select a book to open the reader.</p>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { apiService } from '../services/api'
import { useUnifiedReader } from '../modules/reader/useUnifiedReader'

const books = ref([])
const booksLoading = ref(false)
const booksError = ref(null)
const selectedBook = ref(null)
const sidebarCollapsed = ref(false)
const searchQuery = ref('')
const readerCapabilities = ref(null)
const zimSearchQuery = ref('')
const zimSearchLoading = ref(false)
const zimSearchResults = ref([])
const zimSearchRan = ref(false)

const {
  activeFormat,
  status: unifiedReaderStatus,
  error: readerError,
  epubBook,
  markdownHtml,
  zimMeta,
  zimAdapter,
  zimNativeArticle,
  hasExtension,
  selectBook: selectWithUnifiedReader,
  loadNativeZimArticle,
  resize: resizeUnifiedReader,
  dispose: disposeUnifiedReader
} = useUnifiedReader()

const isEpubSelected = computed(() => {
  return activeFormat.value === 'epub' && epubBook.value
})

const isMarkdownSelected = computed(() => {
  return activeFormat.value === 'md'
})

const isPdfSelected = computed(() => {
  return hasExtension(selectedBook.value?.filename, '.pdf')
})

const filteredBooks = computed(() => {
  return books.value.filter(book =>
    getDisplayName(book.filename).toLowerCase().includes(searchQuery.value.toLowerCase())
  )
})

const selectedZimUrl = computed(() => {
  if (!hasExtension(selectedBook.value?.filename, '.zim')) return ''
  return `/docs/books/${encodeURIComponent(selectedBook.value.filename)}`
})

const selectedPdfUrl = computed(() => {
  if (!hasExtension(selectedBook.value?.filename, '.pdf')) return ''
  return `/data/books/${encodeURIComponent(selectedBook.value.filename)}`
})

const shouldUseNativeZimAdapter = computed(() => {
  return activeFormat.value === 'zim' && zimAdapter.value?.supports_native_render === true
})

const decodePathDeep = (value, maxRounds = 3) => {
  let out = String(value || '')
  for (let i = 0; i < maxRounds; i += 1) {
    try {
      const decoded = decodeURIComponent(out)
      if (decoded === out) break
      out = decoded
    } catch {
      break
    }
  }
  return out
}

const nativeZimUnavailableMessage = computed(() => {
  if (activeFormat.value !== 'zim') {
    return ''
  }

  return 'Native ZIM parsing is unavailable for this archive with the current parser implementation.'
})

const normalizePathKey = (value) => {
  return decodePathDeep(String(value || ''))
    .trim()
    .replace(/^\/+/, '')
    .toLowerCase()
}

const isCurrentSearchResult = (path) => {
  const current = normalizePathKey(zimNativeArticle.value?.path)
  if (!current) {
    return false
  }
  return normalizePathKey(path) === current
}

const onNativeZimArticleClick = async (event) => {
  const anchor = event.target?.closest?.('a')
  if (!anchor) return

  const rawHref = String(anchor.getAttribute('href') || '')
  if (!rawHref || rawHref.startsWith('#')) return

  const lowerHref = rawHref.toLowerCase()
  if (lowerHref.startsWith('mailto:') || lowerHref.startsWith('javascript:')) {
    return
  }

  let resolved
  try {
    resolved = new URL(rawHref, window.location.origin)
  } catch {
    return
  }

  if (resolved.origin !== window.location.origin) {
    return
  }

  event.preventDefault()

  if (!selectedBook.value?.filename) {
    return
  }

  const articlePath = `${decodePathDeep(resolved.pathname)}${resolved.search}`
  if (!articlePath || articlePath === '/') {
    return
  }

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
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i]
}

const getDisplayName = (filename) => {
  return filename.replace(/\.[^/.]+$/, '')
}

const toggleSidebar = () => {
  sidebarCollapsed.value = !sidebarCollapsed.value
  requestAnimationFrame(() => {
    resizeUnifiedReader()
  })
}

const selectBook = async (book) => {
  selectedBook.value = book
  zimSearchQuery.value = ''
  zimSearchResults.value = []
  zimSearchRan.value = false
  await selectWithUnifiedReader(book, apiService)
}

const loadReaderCapabilities = async () => {
  try {
    readerCapabilities.value = await apiService.getReaderCapabilities()
  } catch (error) {
    console.warn('Could not load unified reader capabilities:', error)
    readerCapabilities.value = null
  }
}

const loadBooks = async () => {
  booksLoading.value = true
  booksError.value = null
  try {
    const response = await apiService.getBooks()
    books.value = response.data || []
  } catch (err) {
    console.error('Error loading books:', err)
    booksError.value = apiService.handleError(err)
  } finally {
    booksLoading.value = false
  }
}

onMounted(async () => {
  await loadReaderCapabilities()
  await loadBooks()
})

onBeforeUnmount(() => {
  disposeUnifiedReader()
})
</script>

<style scoped>
.books-page {
  display: flex;
  flex-direction: column;
}

.books-container {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 1rem;
  min-height: calc(100vh - 260px);
}

.books-sidebar,
.book-viewer {
  background: #2a2a2a;
  padding: 1.25rem;
  border-radius: 8px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  border: 1px solid #3a3a3a;
}

.books-sidebar {
  width: 320px;
  transition: width 0.2s ease, padding 0.2s ease;
}

.books-sidebar.collapsed {
  width: 64px;
  padding: 0.65rem;
}

.sidebar-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  margin-bottom: 0.85rem;
}

.sidebar-header h3 {
  margin: 0;
}

.collapse-btn {
  width: 32px;
  height: 32px;
  border-radius: 6px;
  border: 1px solid #4a4a4a;
  background: #1a1a1a;
  color: #d8d8d8;
  cursor: pointer;
}

.search-box {
  margin-bottom: 1rem;
}

.search-input {
  width: 100%;
  padding: 0.75rem;
  background: #1a1a1a;
  border: 1px solid #3a3a3a;
  border-radius: 4px;
  font-size: 0.95rem;
  color: #e0e0e0;
}

.search-input::placeholder {
  color: #808080;
}

.search-input:focus {
  outline: none;
  border-color: #667eea;
  box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.books-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  max-height: calc(100vh - 360px);
  overflow-y: auto;
}

.book-item {
  padding: 0.75rem;
  background: #1a1a1a;
  border: 2px solid #3a3a3a;
  border-radius: 4px;
  cursor: pointer;
  transition: all 0.3s;
  text-align: left;
  display: flex;
  align-items: center;
  gap: 0.75rem;
  color: #e0e0e0;
}

.book-item:hover {
  background: #252525;
  border-color: #667eea;
}

.book-item.active {
  background: #2d3f5a;
  border-color: #667eea;
}

.book-icon {
  font-size: 1.2rem;
}

.book-details {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.book-name {
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.book-size {
  font-size: 0.8rem;
  color: #808080;
}

.empty-state {
  color: #808080;
  font-style: italic;
  text-align: center;
  padding: 1rem;
}

.empty-state a {
  color: #667eea;
  text-decoration: none;
}

.empty-state a:hover {
  text-decoration: underline;
}

.status-state,
.error-state {
  border-radius: 6px;
  padding: 0.65rem 0.75rem;
  font-size: 0.86rem;
}

.hint-text {
  margin: 0 0 0.75rem;
  color: #a8a8a8;
  font-size: 0.82rem;
}

.status-state {
  background: rgba(102, 126, 234, 0.18);
  border: 1px solid rgba(142, 162, 255, 0.45);
  color: #dbe2ff;
}

.error-state {
  background: rgba(164, 45, 45, 0.22);
  border: 1px solid rgba(220, 112, 112, 0.5);
  color: #ffd3d3;
}

.book-content {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  min-height: calc(100vh - 320px);
}

.book-info-empty {
  background: #222;
  border: 1px solid #3a3a3a;
  border-radius: 4px;
  color: #a8a8a8;
  padding: 1rem;
}

.markdown-reader {
  width: 100%;
  min-height: calc(100vh - 320px);
  background: #ffffff;
  border: 1px solid #cfcfcf;
  border-radius: 4px;
  overflow-y: auto;
  overflow-x: hidden;
}

.markdown-content {
  max-width: 900px;
  margin: 0 auto;
  padding: 1.5rem;
  color: #111111;
  line-height: 1.6;
}

.markdown-content :deep(h1),
.markdown-content :deep(h2),
.markdown-content :deep(h3) {
  margin-top: 1.25rem;
  margin-bottom: 0.5rem;
}

.markdown-content :deep(p),
.markdown-content :deep(li) {
  margin-bottom: 0.6rem;
}

.markdown-content :deep(pre) {
  background: #f3f3f3;
  border: 1px solid #dddddd;
  border-radius: 4px;
  padding: 0.9rem;
  overflow-x: auto;
}

.markdown-content :deep(code) {
  font-family: Consolas, Monaco, monospace;
  background: #f3f3f3;
  padding: 0.1rem 0.3rem;
  border-radius: 3px;
}

.markdown-content :deep(a) {
  color: #2a5bd7;
}

.pdf-reader {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.pdf-reader-frame {
  width: 100%;
  min-height: calc(100vh - 320px);
  border: 1px solid #3a3a3a;
  border-radius: 4px;
  background: #ffffff;
}

.pdf-hint {
  margin: 0;
}

.pdf-hint a {
  color: #dbe2ff;
}

.zim-reader {
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
}

.zim-retry-btn {
  align-self: flex-start;
  border: 1px solid #4f67bb;
  background: #2a3f86;
  color: #edf2ff;
  border-radius: 6px;
  padding: 0.45rem 0.7rem;
  cursor: pointer;
}

.zim-retry-btn:hover {
  filter: brightness(1.08);
}

.zim-reader-frame {
  width: 100%;
  min-height: calc(100vh - 320px);
  border: 1px solid #3a3a3a;
  border-radius: 4px;
  background: #fff;
}

.zim-native-panel {
  padding: 0;
  border: 1px solid #3a3a3a;
  border-radius: 6px;
  background: #1f1f1f;
  overflow: hidden;
}

.zim-native-article {
  width: 100%;
  min-height: calc(100vh - 420px);
  background: #ffffff;
  color: #202122;
  border: none;
  border-radius: 0;
  padding: 1.25rem;
  overflow: auto;
  line-height: 1.62;
  font-family: Georgia, "Times New Roman", Times, serif;
  font-size: 0.98rem;
}

.zim-status-bar {
  background: #1f1f1f;
  border: 1px solid #3a3a3a;
  border-radius: 6px;
  padding: 0.8rem;
}

.zim-search {
  margin-bottom: 0.8rem;
}

.zim-search-row {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 0.5rem;
  margin-bottom: 0.45rem;
}

.zim-search-input {
  width: 100%;
  min-width: 0;
  padding: 0.5rem 0.6rem;
  border: 1px solid #4a4a4a;
  border-radius: 6px;
  background: #151515;
  color: #efefef;
}

.zim-search-btn {
  border: 1px solid #4f67bb;
  background: #2a3f86;
  color: #edf2ff;
  border-radius: 6px;
  padding: 0.45rem 0.7rem;
  cursor: pointer;
}

.zim-search-results {
  display: flex;
  flex-wrap: wrap;
  gap: 0.45rem;
}

.zim-search-result {
  border: 1px solid #3f4f74;
  background: #1b2747;
  color: #e5ecff;
  border-radius: 999px;
  padding: 0.35rem 0.6rem;
  font-size: 0.78rem;
  cursor: pointer;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.zim-search-result.is-current,
.zim-search-result:disabled {
  background: #2f2f2f;
  border-color: #565656;
  color: #b5b5b5;
  cursor: default;
}

.zim-native-article :deep(a) {
  color: #3366cc;
  text-decoration: none;
}

.zim-native-article :deep(a:hover) {
  color: #003399;
  text-decoration: underline;
}

.zim-native-article :deep(p) {
  margin: 0.45rem 0 0.7rem;
}

.zim-native-article :deep(h1),
.zim-native-article :deep(h2),
.zim-native-article :deep(h3),
.zim-native-article :deep(h4) {
  font-family: "Linux Libertine", "Times New Roman", Times, serif;
  font-weight: 500;
  line-height: 1.25;
  border-bottom: 1px solid #eaecf0;
  margin: 1rem 0 0.6rem;
  padding-bottom: 0.15rem;
}

.zim-native-article :deep(ul),
.zim-native-article :deep(ol) {
  margin: 0.55rem 0 0.85rem 1.25rem;
}

.zim-native-article :deep(li) {
  margin-bottom: 0.25rem;
}

.zim-native-article :deep(img) {
  max-width: 100%;
  height: auto;
  display: block;
}

.zim-native-article :deep(figure) {
  margin: 0;
}

.zim-native-article :deep(figcaption) {
  font-size: 0.78rem;
  line-height: 1.3;
  color: #3b3f45;
}

.zim-native-article :deep(table) {
  max-width: 100%;
  border-collapse: collapse;
}

.zim-native-article :deep(td),
.zim-native-article :deep(th) {
  border: 1px solid #d8dde3;
  padding: 0.32rem 0.45rem;
  vertical-align: top;
}

.zim-native-article :deep(table:not(.infobox):not(.vertical-navbox):not(.wikitable)) {
  display: block;
  overflow-x: auto;
}

.zim-native-article :deep(.thumb),
.zim-native-article :deep(.infobox),
.zim-native-article :deep(.gallery) {
  max-width: 100%;
}

.zim-native-article :deep(.thumb) {
  margin: 0.4rem 0 0.75rem;
}

.zim-native-article :deep(.thumb img) {
  border: 1px solid #c8ccd1;
  padding: 2px;
  background: #ffffff;
}

.zim-native-article :deep(.infobox) {
  float: right;
  margin: 0 0 0.8rem 0.9rem;
  font-size: 0.86rem;
  width: min(320px, 100%);
  background: #f8f9fa;
}

.zim-native-article :deep(.infobox td),
.zim-native-article :deep(.infobox th) {
  border-color: #c8ccd1;
}

.zim-native-article :deep(a.item) {
  display: inline-flex;
  width: 172px;
  max-width: 100%;
  margin: 0.32rem;
  border: 1px solid #c8ccd1;
  border-radius: 2px;
  overflow: hidden;
  vertical-align: top;
  color: #202122;
  background: #f8f9fa;
}

.zim-native-article :deep(a.item:hover) {
  border-color: #a2a9b1;
  background: #f1f3f5;
}

.zim-native-article :deep(a.item figure) {
  display: flex;
  flex-direction: column;
  width: 100%;
}

.zim-native-article :deep(a.item img) {
  width: 100%;
  aspect-ratio: 4 / 3;
  object-fit: cover;
}

.zim-native-article :deep(a.item figcaption) {
  padding: 0.42rem 0.5rem;
}

#book-viewer {
  width: 100%;
  height: calc(100vh - 320px);
  background: #ffffff;
  border: 1px solid #cfcfcf;
  border-radius: 4px;
  overflow-y: auto;
  overflow-x: hidden;
}

.empty-view {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: calc(100vh - 320px);
  color: #808080;
  font-size: 1.05rem;
}

@media (max-width: 1024px) {
  .books-container {
    grid-template-columns: 1fr;
    min-height: auto;
  }

  .books-sidebar,
  .books-sidebar.collapsed {
    width: 100%;
    padding: 1rem;
  }

  .books-list {
    max-height: 240px;
  }

  #book-viewer,
  .markdown-reader,
  .pdf-reader-frame,
  .zim-reader-frame,
  .book-content,
  .empty-view {
    height: auto;
    min-height: 520px;
  }
}
</style>
