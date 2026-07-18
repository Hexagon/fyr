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
            <p v-if="zimReaderStatus !== 'idle'" class="status-state">{{ zimReaderStatus }}</p>
            <iframe
              ref="zimIframeRef"
              :key="zimIframeKey"
              class="zim-reader-frame"
              :src="zimReaderUrl"
              title="Kiwix JS reader"
              loading="lazy"
              @load="onZimFrameLoad"
              @error="onZimFrameError"
            ></iframe>
            <p v-if="readerError" class="error-state">{{ readerError }}</p>
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
import { ref, computed, onMounted } from 'vue'
import { apiService } from '../services/api'
import EPub from 'epubjs'
import { marked } from 'marked'
import DOMPurify from 'dompurify'

const books = ref([])
const booksLoading = ref(false)
const booksError = ref(null)
const readerError = ref(null)
const selectedBook = ref(null)
const sidebarCollapsed = ref(false)
const searchQuery = ref('')
const epubBook = ref(null)
const epubRendition = ref(null)
const markdownHtml = ref('')
const zimReaderStatus = ref('idle')
const zimIframeKey = ref(0)
const zimIframeRef = ref(null)
const zimReaderConfig = apiService.getZimReaderConfig()
const zimReaderUrl = ref(zimReaderConfig.localUrl)
const kiwixReaderCapabilities = ref(null)
const pendingZimInjection = ref(false)
const lastInjectedZimUrl = ref('')

const ZIM_LOCAL_READER_URL = zimReaderConfig.localUrl

const hasExtension = (filename, extension) => {
  return String(filename || '').toLowerCase().endsWith(extension)
}

const isEpubSelected = computed(() => {
  return hasExtension(selectedBook.value?.filename, '.epub') && epubBook.value
})

const isMarkdownSelected = computed(() => {
  return hasExtension(selectedBook.value?.filename, '.md')
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
    epubRendition.value?.resize()
  })
}

const selectBook = (book) => {
  readerError.value = null
  selectedBook.value = book
  epubBook.value = null
  epubRendition.value = null
  markdownHtml.value = ''

  if (hasExtension(book.filename, '.zim')) {
    zimReaderStatus.value = 'loading'
    pendingZimInjection.value = true
    openSelectedZimInReader()
  }

  if (hasExtension(book.filename, '.epub')) {
    loadEpub(book)
  }

  if (hasExtension(book.filename, '.md')) {
    loadMarkdown(book)
  }
}

const loadEpub = async (book) => {
  try {
    const url = `/data/books/${encodeURIComponent(book.filename)}`
    const bookObj = new EPub(url)

    await bookObj.ready

    const rendition = bookObj.renderTo('book-viewer', {
      width: '100%',
      height: '100%',
      manager: 'continuous',
      flow: 'scrolled-doc',
      spread: 'none',
      allowScriptedContent: false
    })

    rendition.flow('scrolled-doc')
    rendition.spread('none')

    epubBook.value = bookObj
    epubRendition.value = rendition

    await rendition.display()

    rendition.themes.default({
      html: {
        'background-color': '#ffffff'
      },
      body: {
        'background-color': '#ffffff',
        color: '#111111'
      }
    })
    rendition.themes.select('default')
  } catch (err) {
    console.error('Error loading EPUB:', err)
    readerError.value = apiService.handleError(err)
    epubBook.value = null
    epubRendition.value = null
  }
}

const loadMarkdown = async (book) => {
  try {
    const url = `/data/books/${encodeURIComponent(book.filename)}`
    const response = await fetch(url, {
      method: 'GET',
      cache: 'no-store'
    })

    if (!response.ok) {
      throw new Error(`Failed to fetch markdown file (${response.status})`)
    }

    const markdown = await response.text()
    const rendered = marked.parse(markdown)
    markdownHtml.value = DOMPurify.sanitize(rendered, {
      USE_PROFILES: { html: true }
    })
  } catch (err) {
    console.error('Error loading Markdown:', err)
    readerError.value = err?.message || 'Failed to load markdown file.'
    markdownHtml.value = ''
  }
}

const applyEmbeddedKiwixStyles = () => {
  const frameWindow = zimIframeRef.value?.contentWindow
  const frameDocument = frameWindow?.document
  if (!frameDocument) return

  let styleTag = frameDocument.getElementById('fyr-embedded-kiwix-style')
  if (!styleTag) {
    styleTag = frameDocument.createElement('style')
    styleTag.id = 'fyr-embedded-kiwix-style'
    frameDocument.head.appendChild(styleTag)
  }

  styleTag.textContent = `
    #top,
    #footer,
    #liConfigureNav,
    #liAboutNav,
    #btnLibrary {
      display: none !important;
    }

    #search-article {
      padding-top: 0 !important;
      overflow: hidden !important;
    }

    .view-content,
    #articleContent {
      margin-top: 0 !important;
      border: 0 !important;
      height: 100% !important;
      min-height: 100% !important;
    }
  `
}

const ensureLocalZimReaderAvailable = async () => {
  const localAvailable = await apiService.checkLocalZimReader()
  if (localAvailable) {
    return true
  }

  zimReaderStatus.value = 'local reader not found at /kiwix/www/index.html'
  return false
}

const loadKiwixReaderCapabilities = async () => {
  try {
    kiwixReaderCapabilities.value = await apiService.getKiwixReaderCapabilities()
  } catch (error) {
    console.warn('Could not load Kiwix reader capabilities:', error)
    kiwixReaderCapabilities.value = null
  }
}

const buildZimReaderUrl = () => {
  const params = [
    'allowInternetAccess=false',
    'sourceVerification=false'
  ]

  return `${ZIM_LOCAL_READER_URL}?${params.join('&')}`
}

const openSelectedZimInReader = async () => {
  if (!hasExtension(selectedBook.value?.filename, '.zim')) return

  const localAvailable = await ensureLocalZimReaderAvailable()
  if (!localAvailable) {
    zimIframeKey.value += 1
    return
  }

  const supportsDirect = kiwixReaderCapabilities.value?.supports_direct_http_zim === true
  if (!supportsDirect) {
    zimReaderStatus.value = 'reader does not expose direct URL injection in this build'
    return
  }

  pendingZimInjection.value = true
  zimReaderStatus.value = `loading ${selectedBook.value.filename} in embedded reader`

  zimReaderUrl.value = buildZimReaderUrl()
  zimIframeKey.value += 1
}

const injectSelectedZimArchive = async (attempt = 0) => {
  if (!pendingZimInjection.value || !selectedZimUrl.value) return

  const frameWindow = zimIframeRef.value?.contentWindow
  if (!frameWindow) {
    if (attempt < 15) {
      setTimeout(() => {
        injectSelectedZimArchive(attempt + 1)
      }, 200)
    }
    return
  }

  const setRemoteArchives = frameWindow.setRemoteArchives
  if (typeof setRemoteArchives !== 'function') {
    if (attempt < 25) {
      zimReaderStatus.value = 'waiting for reader bootstrap'
      setTimeout(() => {
        injectSelectedZimArchive(attempt + 1)
      }, 200)
    } else {
      zimReaderStatus.value = 'reader loaded but URL injection API is unavailable'
    }
    return
  }

  try {
    zimReaderStatus.value = `opening ${selectedBook.value.filename}`
    await setRemoteArchives(selectedZimUrl.value)
    applyEmbeddedKiwixStyles()
    pendingZimInjection.value = false
    lastInjectedZimUrl.value = selectedZimUrl.value
    zimReaderStatus.value = `opened ${selectedBook.value.filename}`
  } catch (error) {
    console.error('Failed to inject remote ZIM URL:', error)
    zimReaderStatus.value = `failed to open ${selectedBook.value.filename}`
    readerError.value = 'Failed to open ZIM archive in embedded reader.'
  }
}

const onZimFrameLoad = () => {
  applyEmbeddedKiwixStyles()
  if (hasExtension(selectedBook.value?.filename, '.zim')) {
    if (lastInjectedZimUrl.value !== selectedZimUrl.value) {
      pendingZimInjection.value = true
    }
    zimReaderStatus.value = `reader ready for ${selectedBook.value.filename}`
    injectSelectedZimArchive()
    return
  }
  zimReaderStatus.value = 'ready'
}

const onZimFrameError = () => {
  zimReaderStatus.value = 'reader failed to load (check /kiwix assets)'
  readerError.value = 'Embedded reader failed to load. Verify /kiwix assets are available.'
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
  await loadKiwixReaderCapabilities()
  await ensureLocalZimReaderAvailable()
  await loadBooks()
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
  gap: 0.75rem;
}

.zim-reader-frame {
  width: 100%;
  min-height: calc(100vh - 320px);
  border: 1px solid #3a3a3a;
  border-radius: 4px;
  background: #fff;
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
