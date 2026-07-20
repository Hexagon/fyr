<template>
  <div class="content-manager-page">
    <div class="manager-layout">
      <aside class="folder-sidebar" :class="{ collapsed: sidebarCollapsed }">
        <div class="folder-header">
          <h3 v-if="!sidebarCollapsed">Folders</h3>
          <button class="collapse-btn" @click="sidebarCollapsed = !sidebarCollapsed">
            {{ sidebarCollapsed ? '»' : '«' }}
          </button>
        </div>

        <div v-if="!sidebarCollapsed" class="folder-list">
          <button
            v-for="entry in folderEntries"
            :key="entry.key"
            class="folder-item"
            :class="{ active: activeCategory === entry.key }"
            :title="entry.hint"
            @click="activeCategory = entry.key"
          >
            <span>{{ entry.icon }} {{ entry.label }}</span>
            <span class="count">{{ entry.count }}</span>
          </button>
        </div>
      </aside>

      <section class="manager-main">
        <div class="toolbar">
          <div class="search-wrap">
            <input v-model="searchQuery" type="text" placeholder="Search files..." class="search-input" />
            <select v-model="sortBy" class="sort-select">
              <option value="name">Sort: Name</option>
              <option value="size">Sort: Size</option>
              <option value="modified">Sort: Modified</option>
            </select>
            <button class="btn btn-secondary" @click="sortDir = sortDir === 'asc' ? 'desc' : 'asc'">
              {{ sortDir === 'asc' ? 'Asc' : 'Desc' }}
            </button>
          </div>

        </div>

        <div class="file-table-wrap">
          <p v-if="contentError" class="error-text">{{ contentError }}</p>
          <table class="file-table" v-else-if="visibleFiles.length">
            <thead>
              <tr>
                <th>Name</th>
                <th>Size</th>
                <th>Modified</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="file in visibleFiles" :key="`${file.filename}-${file.path}`">
                <td>{{ file.filename }}</td>
                <td>{{ formatBytes(file.size || 0) }}</td>
                <td>{{ formatDate(file.modified) }}</td>
                <td class="actions-cell">
                  <a
                    class="btn btn-secondary btn-inline"
                    :href="buildContentDownloadUrl(activeCategory, file.filename)"
                    :download="file.filename"
                  >
                    Download
                  </a>
                  <button class="btn btn-danger btn-inline" @click="requestDeleteContentFile(file)">Delete</button>
                </td>
              </tr>
            </tbody>
          </table>
          <p v-else class="empty-state">No files in {{ currentFolderLabel }}.</p>
        </div>

        <div class="manager-panels">
          <div class="imports-panel">
            <div class="panel-header">
              <h3>Local Imports</h3>
            </div>

            <input
              ref="fileInput"
              type="file"
              class="hidden-input"
              multiple
              @change="onFilePicked"
            />

            <div class="panel-actions">
              <button class="btn btn-primary" @click="openFilePicker" :disabled="importing">
                {{ importing ? 'Importing...' : 'Import Files' }}
              </button>
            </div>

            <div
              class="drop-zone"
              :class="{ active: dragActive, disabled: importing }"
              @dragenter.prevent="onDragEnter"
              @dragover.prevent="onDragOver"
              @dragleave.prevent="onDragLeave"
              @drop.prevent="onDrop"
            >
              Drop files here to import into {{ currentFolderLabel }}
              <br />
              <small>Supported in this folder: {{ currentFolderHint }}</small>
            </div>

            <p v-if="importStatus" class="status-text">{{ importStatus }}</p>
            <p v-if="importError" class="error-text">{{ importError }}</p>
          </div>

          <div class="downloads-panel">
            <div class="download-header">
              <h3>Download Manager</h3>
              <span class="pill">{{ downloads.length }}</span>
            </div>

            <div v-if="downloads.length" class="download-list">
              <div v-for="dl in downloads" :key="dl.id" class="download-item">
                <p class="download-name">{{ describeDownloadSource(dl.source) }}</p>
                <p class="download-status">
                  <span class="badge" :class="downloadBadgeClass(dl.status)">{{ dl.status }}</span>
                </p>
                <p class="download-progress" v-if="showDownloadProgress(dl)">
                  {{ formatDownloadProgress(dl) }}
                </p>
                <button
                  v-if="isDownloadCancellable(dl.status)"
                  class="btn btn-secondary btn-inline"
                  @click="cancelDownload(dl.id)"
                >
                  Cancel
                </button>
                <button
                  v-if="isDownloadDismissible(dl.status)"
                  class="btn btn-secondary btn-inline"
                  @click="dismissDownload(dl.id)"
                >
                  Dismiss
                </button>
                <p v-if="dl.error" class="error-text">{{ dl.error }}</p>
              </div>
            </div>
            <p v-else-if="!downloadsLoading" class="empty-state">No download tasks</p>
            <p v-if="downloadsLoading" class="status-text">Refreshing downloads...</p>
            <p v-if="downloadsError" class="error-text">{{ downloadsError }}</p>

            <div class="download-create">
              <p class="status-text">
                URL downloads auto-route by extension: maps (.pmtiles), books (.epub, .pdf, .mobi, .md, .zim), POI (.geojson, .json, .fgb), models (.gguf), misc (.txt, .csv, .zip, .7z, .log, installers).
              </p>
              <input
                type="text"
                v-model="downloadUrl"
                placeholder="Enter URL (https://example.com/file.pmtiles)"
                @keyup.enter="handleDownload"
              />
              <button @click="handleDownload" class="btn btn-primary" :disabled="!downloadUrl || urlDownloadPending">
                {{ urlDownloadPending ? 'Downloading...' : 'Download URL' }}
              </button>
            </div>

            <p v-if="urlDownloadStatus" class="status-text">{{ urlDownloadStatus }}</p>
            <p v-if="urlDownloadError" class="error-text">{{ urlDownloadError }}</p>
          </div>
        </div>
      </section>
    </div>

    <div v-if="loading" class="loading">Loading content...</div>

    <div v-if="confirmDeleteFile" class="confirm-overlay">
      <div class="confirm-dialog">
        <p class="confirm-warning">⚠️ Permanent deletion</p>
        <p class="confirm-message">
          Are you sure you want to permanently delete
          <strong>{{ confirmDeleteFile.filename }}</strong>?
          This action cannot be undone.
        </p>
        <p v-if="deleteFileError" class="error-text">{{ deleteFileError }}</p>
        <div class="confirm-actions">
          <button class="btn btn-secondary" :disabled="deleteFilePending" @click="cancelDeleteContentFile">Cancel</button>
          <button class="btn btn-danger" :disabled="deleteFilePending" @click="confirmDeleteContentFile">
            {{ deleteFilePending ? 'Deleting...' : 'Delete permanently' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { useRoute } from 'vue-router'
import { apiService } from '../services/api'

const sidebarCollapsed = ref(false)
const fileInput = ref(null)
const dragActive = ref(false)
const route = useRoute()

const activeCategory = ref('books')
const searchQuery = ref('')
const sortBy = ref('name')
const sortDir = ref('asc')

const downloadUrl = ref('')
const urlDownloadPending = ref(false)
const urlDownloadStatus = ref(null)
const urlDownloadError = ref(null)
const importing = ref(false)
const importStatus = ref(null)
const importError = ref(null)

const maps = ref([])
const books = ref([])
const pois = ref([])
const models = ref([])
const misc = ref([])
const downloads = ref([])
const loading = ref(true)
const contentError = ref(null)
const downloadsError = ref(null)
const downloadsLoading = ref(false)
const confirmDeleteFile = ref(null)
const deleteFileError = ref(null)

let downloadRefreshTimer = null
let hasLoadedDownloads = false
let lastDownloadStateSnapshot = new Map()

const folderEntries = computed(() => [
  { key: 'maps', label: 'Maps', icon: '🗺️', count: maps.value.length, hint: 'Maps accepts .pmtiles files.' },
  { key: 'books', label: 'Books', icon: '📚', count: books.value.length, hint: 'Books accepts .epub, .pdf, .mobi, .md, and .zim files.' },
  { key: 'poi', label: 'POI', icon: '📍', count: pois.value.length, hint: 'POI accepts .geojson, .json, and .fgb files.' },
  { key: 'models', label: 'Models', icon: '🤖', count: models.value.length, hint: 'Models accepts .gguf files (import flow).' },
  { key: 'misc', label: 'Misc', icon: '📦', count: misc.value.length, hint: 'Misc stores general files such as .txt, .csv, .zip, .7z, .log, and installer packages.' }
])

const currentFolderLabel = computed(() => {
  const item = folderEntries.value.find(entry => entry.key === activeCategory.value)
  return item?.label || activeCategory.value
})

const currentFolderHint = computed(() => {
  const item = folderEntries.value.find(entry => entry.key === activeCategory.value)
  return item?.hint || 'No folder hint available.'
})

const filesByCategory = computed(() => ({
  maps: maps.value,
  books: books.value,
  poi: pois.value,
  models: models.value,
  misc: misc.value
}))

const visibleFiles = computed(() => {
  const base = filesByCategory.value[activeCategory.value] || []
  const filtered = base.filter(file =>
    (file.filename || '').toLowerCase().includes(searchQuery.value.toLowerCase())
  )

  const sorted = [...filtered].sort((a, b) => {
    if (sortBy.value === 'size') {
      return (a.size || 0) - (b.size || 0)
    }

    if (sortBy.value === 'modified') {
      return new Date(a.modified || 0).getTime() - new Date(b.modified || 0).getTime()
    }

    return (a.filename || '').localeCompare(b.filename || '')
  })

  return sortDir.value === 'asc' ? sorted : sorted.reverse()
})

const formatBytes = (bytes) => {
  const value = Number(bytes)
  if (!Number.isFinite(value) || value <= 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.min(sizes.length - 1, Math.floor(Math.log(value) / Math.log(k)))
  return Math.round((value / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i]
}

const formatDate = (value) => {
  if (!value) return 'n/a'
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) return 'n/a'
  return d.toLocaleString()
}

const normalizeCategory = (value) => {
  const category = String(value || '').toLowerCase()
  return folderEntries.value.some((entry) => entry.key === category) ? category : null
}

const encodeDownloadSegment = (value) => encodeURIComponent(String(value || '').trim())

const buildContentDownloadUrl = (category, filename) => {
  return `/api/content/${encodeDownloadSegment(String(category || '').toLowerCase())}/${encodeDownloadSegment(filename)}/download`
}

const handleDownload = async () => {
  if (!downloadUrl.value) return

  urlDownloadPending.value = true
  urlDownloadStatus.value = 'Starting download...'
  urlDownloadError.value = null

  try {
    const response = await apiService.createDownload(downloadUrl.value)
    urlDownloadStatus.value = `Download queued: ${response.data.task_id}`
    downloadUrl.value = ''
    await loadDownloads()
  } catch (err) {
    urlDownloadError.value = apiService.handleError(err)
  } finally {
    urlDownloadPending.value = false
  }
}

const isDownloadCancellable = (status) => ['queued', 'downloading', 'validating', 'routing'].includes(String(status || '').toLowerCase())

const isDownloadDismissible = (status) => ['completed', 'failed', 'cancelled'].includes(String(status || '').toLowerCase())

const cancelDownload = async (taskId) => {
  try {
    await apiService.cancelDownload(taskId)
    urlDownloadStatus.value = `Cancelled download: ${taskId}`
    urlDownloadError.value = null
    await loadDownloads()
  } catch (err) {
    urlDownloadError.value = apiService.handleError(err)
  }
}

const dismissDownload = async (taskId) => {
  try {
    await apiService.dismissDownload(taskId)
    await loadDownloads()
  } catch (err) {
    urlDownloadError.value = apiService.handleError(err)
  }
}

const requestDeleteContentFile = (file) => {
  deleteFileError.value = null
  confirmDeleteFile.value = file
}

const deleteFilePending = ref(false)

const confirmDeleteContentFile = async () => {
  const file = confirmDeleteFile.value
  if (!file) return
  deleteFilePending.value = true
  deleteFileError.value = null
  try {
    await apiService.deleteContentFile(activeCategory.value, file.filename)
    confirmDeleteFile.value = null
    await loadContent()
  } catch (err) {
    deleteFileError.value = apiService.handleError(err)
  } finally {
    deleteFilePending.value = false
  }
}

const cancelDeleteContentFile = () => {
  confirmDeleteFile.value = null
  deleteFileError.value = null
}

const openFilePicker = () => {
  fileInput.value?.click()
}

const fileListFromInput = (files) => Array.from(files || []).filter(Boolean)

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

const setCategoryFromContentType = (contentType) => {
  const typeToCategory = {
    map: 'maps',
    book: 'books',
    poi: 'poi',
    model: 'models',
    misc: 'misc'
  }
  const nextCategory = typeToCategory[String(contentType || '').toLowerCase()]
  if (nextCategory) {
    activeCategory.value = nextCategory
  }
}

const importLocalFile = async (file, index, total) => {
  importStatus.value = total > 1
    ? `Uploading ${index + 1} of ${total}: ${file.name}...`
    : `Uploading ${file.name}...`

  const uploadResponse = await apiService.uploadFile(file)
  const uploadedFilename = uploadResponse.data?.filename

  if (!uploadedFilename) {
    throw new Error('Upload did not return a filename.')
  }

  importStatus.value = total > 1
    ? `Queued import ${index + 1} of ${total}: ${uploadedFilename}...`
    : `Queued import for ${uploadedFilename}...`

  const importResponse = await apiService.createImportDownload(uploadedFilename)
  const taskId = importResponse.data?.task_id

  if (!taskId) {
    throw new Error('Import task could not be created.')
  }

  await loadDownloads()

  for (let i = 0; i < 120; i += 1) {
    const statusResponse = await apiService.getDownloadStatus(taskId)
    const task = statusResponse.data
    const status = String(task?.status || '').toLowerCase()

    if (status === 'completed') {
      setCategoryFromContentType(task?.content_type)
      importStatus.value = total > 1
        ? `Imported ${index + 1} of ${total}: ${uploadedFilename}.`
        : `Imported ${uploadedFilename} successfully.`
      await loadDownloads()
      return
    }

    if (status === 'failed' || status === 'cancelled') {
      throw new Error(task?.error || `Import ended with status: ${status}`)
    }

    await sleep(1000)
  }

  throw new Error('Import timed out while waiting for task completion.')
}

const onFilePicked = async (event) => {
  try {
    await importFiles(fileListFromInput(event.target?.files))
  } finally {
    if (event.target) {
      event.target.value = ''
    }
  }
}

const onDrop = async (event) => {
  dragActive.value = false
  await importFiles(fileListFromInput(event.dataTransfer?.files))
}

const onDragEnter = () => {
  if (!importing.value) {
    dragActive.value = true
  }
}

const onDragOver = () => {
  if (!importing.value) {
    dragActive.value = true
  }
}

const onDragLeave = () => {
  dragActive.value = false
}

const importFiles = async (files) => {
  if (!files.length || importing.value) return

  importing.value = true
  importError.value = null

  try {
    for (const [index, file] of files.entries()) {
      await importLocalFile(file, index, files.length)
    }

    await loadContent()
  } catch (err) {
    importError.value = apiService.handleError(err)
    importStatus.value = null
  } finally {
    importing.value = false
    dragActive.value = false
  }
}

const describeDownloadSource = (source) => {
  if (!source) return 'Unknown source'
  if (source.url) return source.url
  if (source.path) {
    const path = String(source.path)
    const filename = path.split(/[/\\]/).filter(Boolean).pop()
    return filename ? `Upload: ${filename}` : path
  }
  if (typeof source === 'string') return source
  return 'Unknown source'
}

const downloadBadgeClass = (status) => String(status || '').toLowerCase()

const showDownloadProgress = (task) => {
  const totalBytes = Number(task?.total_bytes)
  const bytesDownloaded = Number(task?.bytes_downloaded)
  const progress = Number(task?.progress)
  return (Number.isFinite(totalBytes) && totalBytes > 0)
    || (Number.isFinite(bytesDownloaded) && bytesDownloaded > 0)
    || (Number.isFinite(progress) && progress > 0)
}

const formatDownloadProgress = (task) => {
  const totalBytes = Number(task?.total_bytes)
  const bytesDownloaded = Number(task?.bytes_downloaded)
  const safeBytesDownloaded = Number.isFinite(bytesDownloaded) && bytesDownloaded > 0 ? bytesDownloaded : 0
  const progress = Math.max(0, Math.min(100, Math.round(Number(task?.progress) || 0)))

  if (Number.isFinite(totalBytes) && totalBytes > 0) {
    const completedBytes = Math.min(safeBytesDownloaded, totalBytes)
    return `${progress}% (${formatBytes(completedBytes)} / ${formatBytes(totalBytes)})`
  }

  if (safeBytesDownloaded > 0) {
    return `${progress}% (${formatBytes(safeBytesDownloaded)})`
  }

  return `${progress}%`
}

const loadContent = async () => {
  contentError.value = null
  try {
    const [mapsRes, booksRes, poisRes, modelsRes, miscRes] = await Promise.all([
      apiService.getMaps(),
      apiService.getBooks(),
      apiService.getPOIs(),
      apiService.getModels(),
      apiService.getMisc()
    ])

    maps.value = mapsRes.data || []
    books.value = booksRes.data || []
    pois.value = poisRes.data || []
    models.value = modelsRes.data || []
    misc.value = miscRes.data || []
  } catch (err) {
    console.error('Error loading content:', err)
    contentError.value = apiService.handleError(err)
  }
}

const loadDownloads = async () => {
  downloadsLoading.value = true
  downloadsError.value = null
  try {
    const response = await apiService.listDownloads()
    const nextDownloads = Array.isArray(response.data) ? response.data : []
    const nextStateSnapshot = new Map(
      nextDownloads.map((task) => [
        task.id,
        `${String(task?.status || '').toLowerCase()}|${String(task?.content_type || '').toLowerCase()}|${task?.error || ''}`
      ])
    )
    const shouldRefreshContent = hasLoadedDownloads && (
      nextStateSnapshot.size !== lastDownloadStateSnapshot.size
      || Array.from(nextStateSnapshot.entries()).some(([taskId, state]) => lastDownloadStateSnapshot.get(taskId) !== state)
    )

    downloads.value = nextDownloads
    lastDownloadStateSnapshot = nextStateSnapshot
    hasLoadedDownloads = true

    if (shouldRefreshContent) {
      await loadContent()
    }
  } catch (err) {
    console.error('Error loading downloads:', err)
    downloadsError.value = apiService.handleError(err)
  } finally {
    downloadsLoading.value = false
  }
}

const hasLiveDownloads = () => {
  return downloads.value.some((item) => isDownloadCancellable(item.status))
}

const scheduleDownloadRefresh = () => {
  if (downloadRefreshTimer) {
    clearTimeout(downloadRefreshTimer)
  }

  const delayMs = hasLiveDownloads() ? 2000 : 8000
  downloadRefreshTimer = setTimeout(async () => {
    await loadDownloads()
    scheduleDownloadRefresh()
  }, delayMs)
}

watch(
  () => route.query.category,
  (value) => {
    const category = normalizeCategory(value)
    if (category) {
      activeCategory.value = category
    }
  },
  { immediate: true }
)

onMounted(async () => {
  loading.value = true
  await Promise.all([loadContent(), loadDownloads()])
  loading.value = false
  scheduleDownloadRefresh()
})

onUnmounted(() => {
  if (downloadRefreshTimer) {
    clearTimeout(downloadRefreshTimer)
    downloadRefreshTimer = null
  }
})
</script>

<style scoped>
.content-manager-page {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.manager-layout {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 1rem;
  min-height: calc(100vh - 260px);
}

.folder-sidebar,
.manager-main {
  background: #2a2a2a;
  border: 1px solid #3a3a3a;
  border-radius: 8px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
}

.folder-sidebar {
  width: 260px;
  padding: 0.9rem;
  transition: width 0.2s ease, padding 0.2s ease;
}

.folder-sidebar.collapsed {
  width: 64px;
  padding: 0.65rem;
}

.folder-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.8rem;
}

.folder-header h3 {
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

.folder-list {
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
}

.folder-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  border: 1px solid #444;
  background: #1a1a1a;
  color: #dcdcdc;
  border-radius: 6px;
  padding: 0.55rem 0.6rem;
  cursor: pointer;
}

.folder-item.active {
  border-color: #77b255;
  background: #253025;
}

.folder-item .count {
  color: #a8a8a8;
  font-size: 0.85rem;
}

.manager-main {
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.8rem;
  flex-wrap: wrap;
}

.search-wrap {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.search-input,
.sort-select,
.download-create input {
  background: #1a1a1a;
  color: #e0e0e0;
  border: 1px solid #3a3a3a;
  border-radius: 6px;
  padding: 0.55rem 0.65rem;
}

.search-input {
  min-width: 260px;
}

.hidden-input {
  display: none;
}

.manager-panels {
  display: grid;
  grid-template-columns: minmax(280px, 1fr) minmax(320px, 1.2fr);
  gap: 0.85rem;
}

.imports-panel,
.downloads-panel {
  border: 1px solid #3a3a3a;
  border-radius: 8px;
  padding: 0.85rem;
  background: #1f1f1f;
}

.panel-header,
.download-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.5rem;
}

.panel-header h3,
.download-header h3 {
  margin: 0;
  font-size: 1rem;
}

.panel-actions {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.75rem;
}

.drop-zone {
  border: 1px dashed #5a5a5a;
  border-radius: 8px;
  padding: 0.85rem;
  color: #b8b8b8;
  text-align: center;
}

.drop-zone.active {
  border-color: #77b255;
  color: #cbf5cb;
  background: rgba(77, 122, 77, 0.2);
}

.drop-zone.disabled {
  opacity: 0.65;
}

.file-table-wrap {
  border: 1px solid #3a3a3a;
  border-radius: 8px;
  overflow: auto;
  background: #1a1a1a;
}

.file-table {
  width: 100%;
  border-collapse: collapse;
}

.file-table th,
.file-table td {
  padding: 0.65rem;
  border-bottom: 1px solid #343434;
  text-align: left;
}

.file-table th {
  background: #202020;
  color: #cfcfcf;
}

.pill {
  border-radius: 999px;
  border: 1px solid #4d4d4d;
  padding: 0.2rem 0.5rem;
  font-size: 0.8rem;
}

.download-list {
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
  margin-bottom: 0.8rem;
}

.download-item {
  background: #151515;
  border: 1px solid #343434;
  border-radius: 6px;
  padding: 0.55rem;
}

.download-name,
.download-status,
.download-progress {
  margin: 0;
  font-size: 0.85rem;
}

.download-name {
  word-break: break-all;
}

.badge {
  display: inline-block;
  padding: 0.2rem 0.45rem;
  border-radius: 999px;
  font-size: 0.75rem;
  font-weight: 700;
}

.badge.queued {
  background: #3d3a2a;
  color: #d4a94a;
}

.badge.downloading,
.badge.validating,
.badge.routing {
  background: #2a3d3d;
  color: #5cadc4;
}

.badge.completed {
  background: #2d5a2d;
  color: #90ee90;
}

.badge.failed {
  background: #3d2a2a;
  color: #ff6b6b;
}

.badge.cancelled {
  background: #3a3a3a;
  color: #b0b0b0;
}

.download-create {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 0.5rem;
}

.download-create .status-text {
  grid-column: 1 / -1;
  margin: 0;
}

.btn {
  border: none;
  border-radius: 8px;
  padding: 0.55rem 0.8rem;
  font-weight: 600;
  cursor: pointer;
}

.btn-primary {
  background: #6291ff;
  color: #fff;
}

.btn-secondary {
  background: #414141;
  color: #f0f0f0;
}

.status-text {
  color: #cfcfcf;
  font-size: 0.85rem;
}

.error-text {
  color: #ff8e8e;
  font-size: 0.85rem;
}

.empty-state,
.loading {
  color: #9d9d9d;
  font-style: italic;
  margin: 0;
}

.btn-danger {
  background: #7a2020;
  color: #ffdada;
}

.btn-danger:hover {
  background: #9e2a2a;
}

.actions-cell {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 0.5rem;
  text-align: right;
  white-space: nowrap;
}

.actions-cell a.btn {
  text-decoration: none;
}

.confirm-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.65);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.confirm-dialog {
  background: #2a2a2a;
  border: 1px solid #5a3a3a;
  border-radius: 10px;
  padding: 1.5rem;
  max-width: 420px;
  width: 90%;
}

.confirm-warning {
  font-weight: 700;
  color: #ff8e8e;
  margin: 0 0 0.5rem;
}

.confirm-message {
  color: #cfcfcf;
  margin: 0 0 1.25rem;
  font-size: 0.95rem;
  line-height: 1.5;
}

.confirm-actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.65rem;
}

@media (max-width: 1024px) {
  .manager-layout {
    grid-template-columns: 1fr;
    min-height: auto;
  }

  .folder-sidebar,
  .folder-sidebar.collapsed {
    width: 100%;
  }

  .download-create {
    grid-template-columns: 1fr;
  }

  .manager-panels {
    grid-template-columns: 1fr;
  }
}
</style>
