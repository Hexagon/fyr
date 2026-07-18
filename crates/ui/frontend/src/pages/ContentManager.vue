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

          <div class="toolbar-actions">
            <input
              ref="fileInput"
              type="file"
              class="hidden-input"
              @change="onFilePicked"
            />
            <button class="btn btn-primary" @click="openFilePicker">Import File</button>
          </div>
        </div>

        <div
          class="drop-zone"
          :class="{ active: dragActive }"
          @dragenter.prevent="dragActive = true"
          @dragover.prevent="dragActive = true"
          @dragleave.prevent="dragActive = false"
          @drop.prevent="onDrop"
        >
          Drop files here to import into {{ currentFolderLabel }}
        </div>

        <div class="file-table-wrap">
          <table class="file-table" v-if="visibleFiles.length">
            <thead>
              <tr>
                <th>Name</th>
                <th>Size</th>
                <th>Modified</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="file in visibleFiles" :key="`${file.filename}-${file.path}`">
                <td>{{ file.filename }}</td>
                <td>{{ formatBytes(file.size || 0) }}</td>
                <td>{{ formatDate(file.modified) }}</td>
              </tr>
            </tbody>
          </table>
          <p v-else class="empty-state">No files in {{ currentFolderLabel }}.</p>
        </div>

        <div class="downloads-panel">
          <div class="download-header">
            <h3>Active Downloads</h3>
            <span class="pill">{{ downloads.length }}</span>
          </div>

          <div v-if="downloads.length" class="download-list">
            <div v-for="dl in downloads" :key="dl.id" class="download-item">
              <p class="download-name">{{ dl.source?.url || dl.source }}</p>
              <p class="download-status">
                <span class="badge" :class="dl.status">{{ dl.status }}</span>
              </p>
              <p class="download-progress" v-if="dl.total_bytes">
                {{ Math.round((dl.bytes_downloaded / dl.total_bytes) * 100) }}%
                ({{ formatBytes(dl.bytes_downloaded) }} / {{ formatBytes(dl.total_bytes) }})
              </p>
            </div>
          </div>
          <p v-else class="empty-state">No active downloads</p>

          <div class="download-create">
            <input
              type="text"
              v-model="downloadUrl"
              placeholder="Enter URL (https://example.com/file.pmtiles)"
              @keyup.enter="handleDownload"
            />
            <button @click="handleDownload" class="btn btn-primary" :disabled="!downloadUrl || downloading">
              {{ downloading ? 'Downloading...' : 'Download URL' }}
            </button>
          </div>

          <p v-if="downloadStatus" class="status-text">{{ downloadStatus }}</p>
          <p v-if="downloadError" class="error-text">{{ downloadError }}</p>
        </div>
      </section>
    </div>

    <div v-if="loading" class="loading">Loading content...</div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { apiService } from '../services/api'

const sidebarCollapsed = ref(false)
const fileInput = ref(null)
const dragActive = ref(false)

const activeCategory = ref('books')
const searchQuery = ref('')
const sortBy = ref('name')
const sortDir = ref('asc')

const downloadUrl = ref('')
const downloading = ref(false)
const downloadStatus = ref(null)
const downloadError = ref(null)

const maps = ref([])
const books = ref([])
const pois = ref([])
const models = ref([])
const misc = ref([])
const downloads = ref([])
const loading = ref(true)

let downloadRefreshTimer = null

const folderEntries = computed(() => [
  { key: 'maps', label: 'Maps', icon: '🗺️', count: maps.value.length },
  { key: 'books', label: 'Books', icon: '📚', count: books.value.length },
  { key: 'poi', label: 'POI', icon: '📍', count: pois.value.length },
  { key: 'models', label: 'Models', icon: '🤖', count: models.value.length },
  { key: 'misc', label: 'Misc', icon: '📦', count: misc.value.length }
])

const currentFolderLabel = computed(() => {
  const item = folderEntries.value.find(entry => entry.key === activeCategory.value)
  return item?.label || activeCategory.value
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
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i]
}

const formatDate = (value) => {
  if (!value) return 'n/a'
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) return 'n/a'
  return d.toLocaleString()
}

const handleDownload = async () => {
  if (!downloadUrl.value) return

  downloading.value = true
  downloadStatus.value = 'Starting download...'
  downloadError.value = null

  try {
    const response = await apiService.createDownload(downloadUrl.value)
    downloadStatus.value = `Download queued: ${response.data.task_id}`
    downloadUrl.value = ''
    setTimeout(loadDownloads, 500)
  } catch (err) {
    downloadError.value = apiService.handleError(err)
  } finally {
    downloading.value = false
  }
}

const openFilePicker = () => {
  fileInput.value?.click()
}

const importLocalModel = async (file) => {
  if (!file.name.toLowerCase().endsWith('.gguf')) {
    downloadError.value = 'Local upload is currently available for GGUF models only.'
    downloadStatus.value = null
    return
  }

  downloading.value = true
  downloadError.value = null
  downloadStatus.value = `Uploading ${file.name}...`

  try {
    const uploadResponse = await apiService.uploadModel(file)
    await apiService.importModel(uploadResponse.data.filename, 'inbox')
    downloadStatus.value = `Imported ${uploadResponse.data.filename} into model library.`
    activeCategory.value = 'models'
    await loadContent()
  } catch (err) {
    downloadError.value = apiService.handleError(err)
    downloadStatus.value = null
  } finally {
    downloading.value = false
  }
}

const onFilePicked = async (event) => {
  const file = event.target?.files?.[0]
  if (!file) return

  await importLocalModel(file)
  if (event.target) {
    event.target.value = ''
  }
}

const onDrop = async (event) => {
  dragActive.value = false
  const file = event.dataTransfer?.files?.[0]
  if (!file) return

  await importLocalModel(file)
}

const loadContent = async () => {
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
  }
}

const loadDownloads = async () => {
  try {
    const response = await apiService.listDownloads()
    downloads.value = response.data || []
  } catch (err) {
    console.error('Error loading downloads:', err)
  }
}

onMounted(async () => {
  loading.value = true
  await Promise.all([loadContent(), loadDownloads()])
  loading.value = false

  downloadRefreshTimer = setInterval(loadDownloads, 2000)
})

onUnmounted(() => {
  if (downloadRefreshTimer) {
    clearInterval(downloadRefreshTimer)
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

.toolbar-actions {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.hidden-input {
  display: none;
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

.downloads-panel {
  border: 1px solid #3a3a3a;
  border-radius: 8px;
  padding: 0.85rem;
  background: #1f1f1f;
}

.download-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.5rem;
}

.download-header h3 {
  margin: 0;
  font-size: 1rem;
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

.badge.Queued {
  background: #3d3a2a;
  color: #d4a94a;
}

.badge.Downloading,
.badge.Validating,
.badge.Routing {
  background: #2a3d3d;
  color: #5cadc4;
}

.badge.Completed {
  background: #2d5a2d;
  color: #90ee90;
}

.badge.Failed {
  background: #3d2a2a;
  color: #ff6b6b;
}

.badge.Cancelled {
  background: #3a3a3a;
  color: #b0b0b0;
}

.download-create {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 0.5rem;
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
}
</style>
