<template>
  <div class="content-manager-page">
    <div class="manager-layout">
      <aside class="folder-sidebar" :class="{ collapsed: sidebarCollapsed }">
        <div class="folder-header">
          <h3 v-if="!sidebarCollapsed">{{ t('content.folders') }}</h3>
          <button class="collapse-btn" :aria-label="sidebarCollapsed ? t('content.expandFolders') : t('content.collapseFolders')" :aria-expanded="String(!sidebarCollapsed)" @click="sidebarCollapsed = !sidebarCollapsed">
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
        <div v-if="loading" class="loading-overlay">
          <p class="loading">{{ t('content.loading') }}</p>
        </div>

        <div class="toolbar">
          <div class="search-wrap">
            <input v-model="searchQuery" type="text" :placeholder="t('content.searchPlaceholder')" :aria-label="t('content.searchPlaceholder')" class="search-input" />
            <select v-model="sortBy" class="sort-select" :aria-label="t('content.sortLabel')">
              <option value="name">{{ t('content.sortName') }}</option>
              <option value="size">{{ t('content.sortSize') }}</option>
              <option value="modified">{{ t('content.sortModified') }}</option>
            </select>
            <button class="btn btn-secondary" @click="sortDir = sortDir === 'asc' ? 'desc' : 'asc'">
              {{ sortDir === 'asc' ? t('content.asc') : t('content.desc') }}
            </button>
          </div>

        </div>

        <div class="file-table-wrap">
          <p v-if="contentError" class="error-text" role="alert" aria-live="assertive">{{ contentError }}</p>
          <p v-if="indexError && activeCategory === 'books'" class="error-text" role="alert">{{ indexError }}</p>
          <p v-if="indexStatus.lowDisk && activeCategory === 'books'" class="status-text">
            {{ t('content.lowDisk') }}
          </p>
          <p v-if="placesError && activeCategory === 'maps'" class="error-text" role="alert">{{ placesError }}</p>
          <p v-if="placesStatus.lowDisk && activeCategory === 'maps'" class="status-text">
            {{ t('content.placesLowDisk') }}
          </p>
          <table class="file-table" v-if="visibleFiles.length">
            <thead>
              <tr>
                <th>{{ t('content.columns.name') }}</th>
                <th>{{ t('content.columns.size') }}</th>
                <th>{{ t('content.columns.modified') }}</th>
                <th v-if="showIndexColumn">{{ t('content.columns.fulltext') }}</th>
                <th v-if="showPlacesColumn">{{ t('content.columns.places') }}</th>
                <th><span class="visually-hidden">{{ t('content.columns.actions') }}</span></th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="file in visibleFiles" :key="`${file.filename}-${file.path}`">
                <td>{{ file.filename }}</td>
                <td>{{ formatBytes(file.size || 0) }}</td>
                <td>{{ formatDate(file.modified) }}</td>
                <td v-if="showIndexColumn" class="index-cell">
                  <template v-if="isZimFilename(file.filename)">
                    <span
                      class="index-pill"
                      :class="`index-${indexInfo(file.filename).tone}`"
                      :title="indexInfo(file.filename).title"
                    >{{ indexInfo(file.filename).label }}</span>
                    <progress
                      v-if="indexInfo(file.filename).showProgress"
                      class="index-progress"
                      max="100"
                      :value="indexInfo(file.filename).percent"
                    ></progress>
                    <span v-if="!adminLocked" class="index-actions">
                      <button
                        v-if="indexInfo(file.filename).canBuild"
                        class="btn btn-secondary btn-inline"
                        :disabled="indexBusy.has(file.filename)"
                        @click="buildFulltextIndex(file.filename)"
                      >{{ t('content.index.build') }}</button>
                      <button
                        v-if="indexInfo(file.filename).canDelete"
                        class="btn btn-secondary btn-inline"
                        :disabled="indexBusy.has(file.filename)"
                        @click="deleteFulltextIndex(file.filename)"
                      >{{ t('content.index.delete') }}</button>
                    </span>
                  </template>
                </td>
                <td v-if="showPlacesColumn" class="index-cell">
                  <template v-if="isMapFilename(file.filename)">
                    <span
                      class="index-pill"
                      :class="`index-${placesInfo(file.filename).tone}`"
                      :title="placesInfo(file.filename).title"
                    >{{ placesInfo(file.filename).label }}</span>
                    <progress
                      v-if="placesInfo(file.filename).showProgress"
                      class="index-progress"
                      max="100"
                      :value="placesInfo(file.filename).percent"
                    ></progress>
                    <span v-if="!adminLocked" class="index-actions">
                      <button
                        v-if="placesInfo(file.filename).canBuild"
                        class="btn btn-secondary btn-inline"
                        :disabled="placesBusy.has(file.filename)"
                        @click="buildPlaceIndex(file.filename)"
                      >{{ t('content.places.build') }}</button>
                      <button
                        v-if="placesInfo(file.filename).canDelete"
                        class="btn btn-secondary btn-inline"
                        :disabled="placesBusy.has(file.filename)"
                        @click="deletePlaceIndex(file.filename)"
                      >{{ t('content.places.delete') }}</button>
                    </span>
                  </template>
                </td>
                <td class="actions-cell">
                  <a
                    class="btn btn-secondary btn-inline"
                    :href="buildContentDownloadUrl(activeCategory, file.filename)"
                    :download="file.filename"
                  >
                    {{ t('content.download') }}
                  </a>
                  <button class="btn btn-danger btn-inline" @click="requestDeleteContentFile(file)">{{ t('common.delete') }}</button>
                </td>
              </tr>
            </tbody>
          </table>
          <div v-else class="empty-panel">
            <p class="empty-state">{{ t('content.noFiles', { folder: currentFolderLabel }) }}</p>
            <p v-if="!showCuratedPanel" class="status-text">{{ t('content.getStarted') }}</p>
            <p v-if="curatedContentError" class="error-text" role="alert" aria-live="assertive">{{ curatedContentError }}</p>
          </div>
        </div>

        <div v-if="showCuratedPanel" class="curated-panel" :class="showCuratedPrimary ? 'curated-panel-primary' : 'curated-panel-secondary'">
          <div class="curated-header">
            <div>
              <h3>{{ curatedSectionTitle }}</h3>
              <p class="status-text">{{ curatedSectionHint }}</p>
            </div>
            <span class="pill">{{ activeCuratedItems.length }}</span>
          </div>

          <div class="curated-list">
            <article v-for="item in activeCuratedItems" :key="item.id" class="curated-item">
              <div class="curated-copy">
                <h4>{{ item.title }}</h4>
                <p v-if="item.description" class="curated-description">{{ item.description }}</p>
                <div v-if="item.meta.length" class="curated-meta">
                  <span v-for="meta in item.meta" :key="`${item.id}-${meta}`" class="curated-chip">{{ meta }}</span>
                </div>
              </div>

              <div class="curated-actions">
                <button
                  v-if="item.downloadUrl"
                  class="btn btn-primary btn-inline"
                  :disabled="urlDownloadPending"
                  @click="startCuratedDownload(item)"
                >
                  {{ activeDownloadUrl === String(item.downloadUrl).trim() ? t('content.downloading') : t('content.downloadNow') }}
                </button>
                <a
                  v-if="item.source"
                  class="btn btn-secondary btn-inline"
                  :href="item.source"
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  {{ t('content.openSource') }}
                </a>
              </div>
            </article>
          </div>
        </div>

        <div class="manager-panels">
          <div class="imports-panel">
            <div class="panel-header">
              <h3>{{ t('content.imports.title') }}</h3>
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
                {{ importing ? t('content.imports.importing') : t('content.imports.button') }}
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
              {{ t('content.imports.drop', { folder: currentFolderLabel }) }}
              <br />
              <small>{{ t('content.imports.supported', { hint: currentFolderHint }) }}</small>
            </div>

            <p v-if="importError" class="error-text" role="alert" aria-live="assertive">{{ importError }}</p>
          </div>

          <div class="downloads-panel">
            <div class="download-header">
              <h3>{{ t('content.downloads.title') }}</h3>
              <span class="pill">{{ pendingUploads.length + downloads.length }}</span>
            </div>

            <div v-if="pendingUploads.length || downloads.length" class="download-list">
              <div v-for="pu in pendingUploads" :key="pu.id" class="download-item">
                <p class="download-name">{{ t('content.downloads.upload', { name: pu.filename }) }}</p>
                <p class="download-status">
                  <span class="badge uploading">{{ downloadStatusLabel('uploading') }}</span>
                </p>
                <div v-if="pu.totalBytes > 0" class="progress-bar-wrap">
                  <div class="progress-bar-fill uploading" :style="{ width: pu.progress + '%' }"></div>
                </div>
                <p class="download-progress" v-if="pu.totalBytes > 0">
                  {{ t('content.downloads.progressOf', { pct: pu.progress, done: formatBytes(pu.bytesSent), total: formatBytes(pu.totalBytes) }) }}
                </p>
                <button class="btn btn-secondary btn-inline" @click="pu.abortFn()">{{ t('common.cancel') }}</button>
              </div>
              <div v-for="dl in downloads" :key="dl.id" class="download-item">
                <p class="download-name">{{ describeDownloadSource(dl.source) }}</p>
                <p class="download-status">
                  <span class="badge" :class="downloadBadgeClass(dl.status)">{{ downloadStatusLabel(dl.status) }}</span>
                </p>
                <div v-if="showDownloadProgress(dl)" class="progress-bar-wrap">
                  <div class="progress-bar-fill" :class="downloadBadgeClass(dl.status)" :style="{ width: Math.max(0, Math.min(100, Math.round(Number(dl.progress) || 0))) + '%' }"></div>
                </div>
                <p class="download-progress" v-if="showDownloadProgress(dl)">
                  {{ formatDownloadProgress(dl) }}
                </p>
                <p v-if="describeResumeState(dl, formatBytes)" class="status-text download-resume">
                  {{ describeResumeState(dl, formatBytes) }}
                </p>
                <button
                  v-if="!adminLocked && canRetryDownload(dl)"
                  class="btn btn-primary btn-inline"
                  :disabled="retryingTaskIds.has(dl.id)"
                  @click="retryDownload(dl.id)"
                >
                  {{ retryingTaskIds.has(dl.id) ? t('content.downloads.retrying') : retryActionLabel(dl) }}
                </button>
                <button
                  v-if="isDownloadCancellable(dl.status)"
                  class="btn btn-secondary btn-inline"
                  @click="cancelDownload(dl.id)"
                >
                  {{ t('common.cancel') }}
                </button>
                <button
                  v-if="isDownloadDismissible(dl.status)"
                  class="btn btn-secondary btn-inline"
                  @click="dismissDownload(dl.id)"
                >
                  {{ t('content.downloads.dismiss') }}
                </button>
                <p v-if="dl.error" class="error-text" role="alert" aria-live="assertive">{{ dl.error }}</p>
              </div>
            </div>
            <p v-else-if="!downloadsLoading" class="empty-state">{{ t('content.downloads.empty') }}</p>
            <p v-if="downloadsLoading" class="status-text" role="status" aria-live="polite">{{ t('content.downloads.refreshing') }}</p>
            <p v-if="downloadsError" class="error-text" role="alert" aria-live="assertive">{{ downloadsError }}</p>

            <div class="download-create">
              <p class="status-text">
                {{ t('content.downloads.routing') }}
              </p>
              <input
                type="text"
                v-model="downloadUrl"
                :placeholder="t('content.downloads.urlPlaceholder')"
                :aria-label="t('content.downloads.urlLabel')"
                @keyup.enter="handleDownload"
              />
              <input
                type="text"
                v-model="downloadSha256"
                class="sha256-input"
                :placeholder="t('content.downloads.shaPlaceholder')"
                :aria-label="t('content.downloads.shaPlaceholder')"
                spellcheck="false"
                autocomplete="off"
                @keyup.enter="handleDownload"
              />
              <button @click="handleDownload" class="btn btn-primary" :disabled="!downloadUrl || urlDownloadPending">
                {{ urlDownloadPending ? t('content.downloading') : t('content.downloads.button') }}
              </button>
            </div>

            <p v-if="urlDownloadStatus" class="status-text" role="status" aria-live="polite">{{ urlDownloadStatus }}</p>
            <p v-if="urlDownloadError" class="error-text" role="alert" aria-live="assertive">{{ urlDownloadError }}</p>
          </div>
        </div>
      </section>
    </div>

    <div v-if="confirmDeleteFile" class="confirm-overlay">
      <div class="confirm-dialog">
        <p class="confirm-warning">⚠️ {{ t('content.confirmDelete.title') }}</p>
        <i18n-t keypath="content.confirmDelete.message" tag="p" class="confirm-message" scope="global">
          <template #name><strong>{{ confirmDeleteFile.filename }}</strong></template>
        </i18n-t>
        <p v-if="deleteFileError" class="error-text" role="alert" aria-live="assertive">{{ deleteFileError }}</p>
        <div class="confirm-actions">
          <button class="btn btn-secondary" :disabled="deleteFilePending" @click="cancelDeleteContentFile">{{ t('common.cancel') }}</button>
          <button class="btn btn-danger" :disabled="deleteFilePending" @click="confirmDeleteContentFile">
            {{ deleteFilePending ? t('content.confirmDelete.deleting') : t('content.confirmDelete.confirm') }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { formatBytes, formatNumber } from '../i18n/index.js'
import { useRoute } from 'vue-router'
import { apiService } from '../services/api'
import { isAdminLocked } from '../services/auth'
import {
  INDEX_POLL_MS,
  describeIndexEntry,
  isZimFilename,
  normalizeIndexStatus,
  shouldPollIndex
} from '../modules/library/fulltextIndex.js'
import {
  PLACES_POLL_MS,
  describePlaceEntry,
  isMapFilename,
  normalizePlaceStatus,
  shouldPollPlaces
} from '../modules/maps/placeIndex.js'
import {
  canRetryDownload,
  describeResumeState,
  isActiveDownloadStatus,
  normalizeSha256Input,
  retryActionLabel
} from '../services/downloadResume'

const sidebarCollapsed = ref(false)
const fileInput = ref(null)
const dragActive = ref(false)
const route = useRoute()

const { t, locale } = useI18n()
const activeCategory = ref('books')
const searchQuery = ref('')
const sortBy = ref('name')
const sortDir = ref('asc')

const downloadUrl = ref('')
const downloadSha256 = ref('')
const retryingTaskIds = ref(new Set())
const adminLocked = computed(() => isAdminLocked())
const urlDownloadPending = ref(false)
const activeDownloadUrl = ref(null)
const urlDownloadStatus = ref(null)
const urlDownloadError = ref(null)
const importing = ref(false)
const importError = ref(null)

const maps = ref([])
const books = ref([])
const pois = ref([])
const models = ref([])
const misc = ref([])
const downloads = ref([])
const pendingUploads = ref([])
const curatedContent = ref({ items: {} })
const loading = ref(true)
const contentError = ref(null)
const curatedContentError = ref(null)
const downloadsError = ref(null)
const downloadsLoading = ref(false)
const confirmDeleteFile = ref(null)
const deleteFileError = ref(null)

let downloadRefreshTimer = null
let indexRefreshTimer = null

const indexStatus = ref(normalizeIndexStatus(null))
const indexError = ref(null)
const indexBusy = ref(new Set())
const showIndexColumn = computed(() => activeCategory.value === 'books' && books.value.some((b) => isZimFilename(b.filename)))
const indexInfo = (filename) =>
  describeIndexEntry(indexStatus.value.byFile.get(filename), { enabled: indexStatus.value.enabled })

const scheduleIndexRefresh = () => {
  if (indexRefreshTimer) clearTimeout(indexRefreshTimer)
  indexRefreshTimer = null
  if (activeCategory.value === 'books' && shouldPollIndex(indexStatus.value)) {
    indexRefreshTimer = setTimeout(loadIndexStatus, INDEX_POLL_MS)
  }
}

const loadIndexStatus = async () => {
  try {
    indexStatus.value = normalizeIndexStatus(await apiService.getIndexStatus())
  } catch (err) {
    // Older servers have no index API: show nothing rather than an error.
    indexStatus.value = normalizeIndexStatus(null)
  }
  scheduleIndexRefresh()
}

const runIndexAction = async (filename, action) => {
  indexError.value = null
  indexBusy.value = new Set([...indexBusy.value, filename])
  try {
    await action(filename)
  } catch (err) {
    indexError.value = apiService.handleError(err)
  } finally {
    const next = new Set(indexBusy.value)
    next.delete(filename)
    indexBusy.value = next
    await loadIndexStatus()
  }
}

const buildFulltextIndex = (filename) => runIndexAction(filename, apiService.buildIndex)
const deleteFulltextIndex = (filename) => runIndexAction(filename, apiService.deleteIndex)

// Place index for vector maps (same pattern as the full-text index above).
let placesRefreshTimer = null
const placesStatus = ref(normalizePlaceStatus(null))
const placesError = ref(null)
const placesBusy = ref(new Set())
const showPlacesColumn = computed(() => activeCategory.value === 'maps' && maps.value.some((m) => isMapFilename(m.filename)))
const placesInfo = (filename) =>
  describePlaceEntry(placesStatus.value.byFile.get(filename), { enabled: placesStatus.value.enabled })

const schedulePlacesRefresh = () => {
  if (placesRefreshTimer) clearTimeout(placesRefreshTimer)
  placesRefreshTimer = null
  if (activeCategory.value === 'maps' && shouldPollPlaces(placesStatus.value)) {
    placesRefreshTimer = setTimeout(loadPlacesStatus, PLACES_POLL_MS)
  }
}

const loadPlacesStatus = async () => {
  try {
    placesStatus.value = normalizePlaceStatus(await apiService.getPlacesStatus())
  } catch (err) {
    // Older servers have no place index API: show nothing rather than an error.
    placesStatus.value = normalizePlaceStatus(null)
  }
  schedulePlacesRefresh()
}

const runPlacesAction = async (filename, action) => {
  placesError.value = null
  placesBusy.value = new Set([...placesBusy.value, filename])
  try {
    await action(filename)
  } catch (err) {
    placesError.value = apiService.handleError(err)
  } finally {
    const next = new Set(placesBusy.value)
    next.delete(filename)
    placesBusy.value = next
    await loadPlacesStatus()
  }
}

const buildPlaceIndex = (filename) => runPlacesAction(filename, apiService.buildPlaces)
const deletePlaceIndex = (filename) => runPlacesAction(filename, apiService.deletePlaces)
let pendingUploadCounter = 0
let hasLoadedDownloads = false
let lastDownloadStateSnapshot = new Map()

const folderEntries = computed(() => [
  { key: 'maps', label: t('content.folder.maps'), icon: '🗺️', count: maps.value.length, hint: t('content.folder.mapsHint') },
  { key: 'books', label: t('content.folder.books'), icon: '📚', count: books.value.length, hint: t('content.folder.booksHint') },
  { key: 'poi', label: t('content.folder.poi'), icon: '📍', count: pois.value.length, hint: t('content.folder.poiHint') },
  { key: 'models', label: t('content.folder.models'), icon: '🤖', count: models.value.length, hint: t('content.folder.modelsHint') },
  { key: 'misc', label: t('content.folder.misc'), icon: '📦', count: misc.value.length, hint: t('content.folder.miscHint') }
])

const currentFolderLabel = computed(() => {
  const item = folderEntries.value.find(entry => entry.key === activeCategory.value)
  return item?.label || activeCategory.value
})

const currentFolderHint = computed(() => {
  const item = folderEntries.value.find(entry => entry.key === activeCategory.value)
  return item?.hint || t('content.folder.noHint')
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

const curatedCatalogCategoryMap = Object.freeze({
  maps: 'maps',
  books: 'books',
  models: 'models'
})

const formatApproximateGigabytes = (value) => {
  const size = Number(value)
  if (!Number.isFinite(size) || size <= 0) return null
  return `~${formatNumber(size)} GB`
}

const formatArchitectureLabel = (value) => {
  const normalized = String(value || '').trim().toLowerCase()
  if (!normalized) return null

  const family = { qwen2: 'Qwen2', llama: 'Llama', phi3: 'Phi-3' }[normalized] || value
  return t('content.curated.family', { family })
}

const buildCuratedMeta = (category, item) => {
  if (category === 'models') {
    return [
      formatArchitectureLabel(item.architecture),
      item.tier ? t('content.curated.tier', { tier: item.tier }) : null,
      item.quantization || null,
      formatApproximateGigabytes(item.approx_size_gb),
      item.license ? t('content.curated.license', { license: item.license }) : null,
      item.default_profile ? t('content.curated.profile', { profile: item.default_profile }) : null,
      typeof item.tested_with_fyr === 'boolean'
        ? (item.tested_with_fyr ? t('content.curated.validated') : t('content.curated.notValidated'))
        : null
    ].filter(Boolean)
  }

  const formats = Array.isArray(item.formats) && item.formats.length
    ? t('content.curated.formats', { formats: item.formats.map((format) => String(format).toUpperCase()).join(', ') })
    : null

  return [formats].filter(Boolean)
}

const normalizedCuratedItems = (category) => {
  const catalogKey = curatedCatalogCategoryMap[category]
  const items = catalogKey ? curatedContent.value?.items?.[catalogKey] : []
  if (!Array.isArray(items)) {
    return []
  }

  return items.map((item, index) => ({
    id: item.id || `${category}-${index}`,
    title: category === 'models'
      ? [item.model, item.quantization].filter(Boolean).join(' • ') || item.id || t('content.curated.recommendedModel')
      : item.label || item.id || t('content.curated.recommendedSource'),
    description: item.recommended_for || item.notes || '',
    downloadUrl: String(item.download_url || '').trim(),
    sha256: String(item.sha256 || '').trim(),
    source: String(item.source || '').trim(),
    meta: buildCuratedMeta(category, item)
  }))
}

const activeCuratedItems = computed(() => normalizedCuratedItems(activeCategory.value))

const showCuratedPrimary = computed(() => !contentError.value && !visibleFiles.value.length && activeCuratedItems.value.length > 0)

const showCuratedSupplement = computed(() => !contentError.value && visibleFiles.value.length > 0 && activeCuratedItems.value.length > 0)

const showCuratedPanel = computed(() => showCuratedPrimary.value || showCuratedSupplement.value)

const curatedSectionTitle = computed(() => showCuratedPrimary.value
  ? t('content.curated.recommendedTitle', { folder: currentFolderLabel.value })
  : t('content.curated.moreTitle', { folder: currentFolderLabel.value }))

const curatedSectionHint = computed(() => showCuratedPrimary.value
  ? t('content.curated.primaryHint', { folder: currentFolderLabel.value.toLowerCase() })
  : t('content.curated.secondaryHint', { folder: currentFolderLabel.value.toLowerCase() }))

const formatDate = (value) => {
  if (!value) return t('common.na')
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) return t('common.na')
  return d.toLocaleString(locale.value)
}

const DOWNLOAD_STATUSES = ['queued', 'downloading', 'validating', 'verifying', 'routing', 'completed', 'failed', 'cancelled', 'uploading']
const downloadStatusLabel = (status) => {
  const key = String(status || '').toLowerCase()
  return DOWNLOAD_STATUSES.includes(key) ? t(`content.status.${key}`) : String(status || '')
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

  const url = downloadUrl.value.trim()
  if (!url.startsWith('http://') && !url.startsWith('https://')) {
    urlDownloadStatus.value = null
    urlDownloadError.value = t('content.errors.urlScheme')
    return
  }

  const checksum = normalizeSha256Input(downloadSha256.value)
  if (checksum.error) {
    urlDownloadStatus.value = null
    urlDownloadError.value = checksum.error
    return
  }

  urlDownloadPending.value = true
  activeDownloadUrl.value = url
  urlDownloadStatus.value = t('content.downloads.starting')
  urlDownloadError.value = null

  try {
    await apiService.createDownload(url, checksum.value)
    urlDownloadStatus.value = checksum.value
      ? t('content.downloads.queuedSha')
      : t('content.downloads.queued')
    downloadUrl.value = ''
    downloadSha256.value = ''
    await loadDownloads()
  } catch (err) {
    urlDownloadError.value = apiService.handleError(err)
  } finally {
    urlDownloadPending.value = false
    activeDownloadUrl.value = null
  }
}

const startCuratedDownload = async (item) => {
  if (!item?.downloadUrl || urlDownloadPending.value) return
  downloadUrl.value = item.downloadUrl
  // Curated entries may carry a sha256; pass it so the server verifies the file.
  downloadSha256.value = item.sha256 || ''
  urlDownloadError.value = null
  await handleDownload()
}

const isDownloadCancellable = (status) => isActiveDownloadStatus(status)

const isDownloadDismissible = (status) => ['completed', 'failed', 'cancelled'].includes(String(status || '').toLowerCase())

const cancelDownload = async (taskId) => {
  try {
    await apiService.cancelDownload(taskId)
    downloadsError.value = null
    await loadDownloads()
  } catch (err) {
    downloadsError.value = apiService.handleError(err)
  }
}

const retryDownload = async (taskId) => {
  if (retryingTaskIds.value.has(taskId)) return
  retryingTaskIds.value = new Set([...retryingTaskIds.value, taskId])
  try {
    await apiService.retryDownload(taskId)
    downloadsError.value = null
    await loadDownloads()
    scheduleDownloadRefresh()
  } catch (err) {
    downloadsError.value = apiService.handleError(err)
  } finally {
    const next = new Set(retryingTaskIds.value)
    next.delete(taskId)
    retryingTaskIds.value = next
  }
}

const dismissDownload = async (taskId) => {
  try {
    await apiService.dismissDownload(taskId)
    downloadsError.value = null
    await loadDownloads()
  } catch (err) {
    downloadsError.value = apiService.handleError(err)
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
  pendingUploadCounter += 1
  const pendingId = `__upload_${pendingUploadCounter}`
  const abortHandle = { abort: null }
  const pending = {
    id: pendingId,
    filename: file.name,
    progress: 0,
    bytesSent: 0,
    totalBytes: file.size || 0,
    abortFn: () => abortHandle.abort?.()
  }
  pendingUploads.value.push(pending)

  try {
    const uploadResponse = await apiService.uploadFile(file, (loaded, total) => {
      pending.bytesSent = loaded
      pending.totalBytes = total
      pending.progress = total > 0 ? Math.round((loaded / total) * 100) : 0
    }, abortHandle)

    const uploadedFilename = uploadResponse.data?.filename

    if (!uploadedFilename) {
      throw new Error(t('content.errors.noFilename'))
    }

    // Upload complete — remove the synthetic entry before the real task appears
    pendingUploads.value = pendingUploads.value.filter((p) => p.id !== pendingId)

    const importResponse = await apiService.createImportDownload(uploadedFilename)
    const taskId = importResponse.data?.task_id

    if (!taskId) {
      throw new Error(t('content.errors.noTask'))
    }

    await loadDownloads()

    for (let i = 0; i < 120; i += 1) {
      const statusResponse = await apiService.getDownloadStatus(taskId)
      const task = statusResponse.data
      const status = String(task?.status || '').toLowerCase()

      if (status === 'completed') {
        setCategoryFromContentType(task?.content_type)
        await loadDownloads()
        return
      }

      if (status === 'failed' || status === 'cancelled') {
        throw new Error(task?.error || t('content.errors.importStatus', { status: downloadStatusLabel(status) }))
      }

      await sleep(1000)
    }

    throw new Error(t('content.errors.importTimeout'))
  } catch (err) {
    pendingUploads.value = pendingUploads.value.filter((p) => p.id !== pendingId)
    throw err
  }
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
  } finally {
    importing.value = false
    dragActive.value = false
  }
}

const describeDownloadSource = (source) => {
  if (!source) return t('content.downloads.unknownSource')
  if (source.url) return source.url
  if (source.path) {
    const path = String(source.path)
    const filename = path.split(/[/\\]/).filter(Boolean).pop()
    return filename ? t('content.downloads.upload', { name: filename }) : path
  }
  if (typeof source === 'string') return source
  return t('content.downloads.unknownSource')
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
    return t('content.downloads.progressOf', { pct: progress, done: formatBytes(completedBytes), total: formatBytes(totalBytes) })
  }

  if (safeBytesDownloaded > 0) {
    return t('content.downloads.progressBytes', { pct: progress, done: formatBytes(safeBytesDownloaded) })
  }

  return t('content.downloads.progress', { pct: progress })
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

const loadCuratedContent = async () => {
  curatedContentError.value = null
  try {
    const response = await apiService.getCuratedContent()
    curatedContent.value = response.data || { items: {} }
  } catch (err) {
    console.error('Error loading curated content:', err)
    curatedContent.value = { items: {} }
    curatedContentError.value = apiService.handleError(err)
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
  await Promise.all([loadContent(), loadDownloads(), loadCuratedContent(), loadIndexStatus(), loadPlacesStatus()])
  loading.value = false
  scheduleDownloadRefresh()
})

watch(activeCategory, (category) => {
  if (category === 'books') loadIndexStatus()
  else scheduleIndexRefresh()
  if (category === 'maps') loadPlacesStatus()
  else schedulePlacesRefresh()
})

onUnmounted(() => {
  if (indexRefreshTimer) {
    clearTimeout(indexRefreshTimer)
    indexRefreshTimer = null
  }
  if (placesRefreshTimer) {
    clearTimeout(placesRefreshTimer)
    placesRefreshTimer = null
  }
  if (downloadRefreshTimer) {
    clearTimeout(downloadRefreshTimer)
    downloadRefreshTimer = null
  }
})
</script>

<style scoped>
.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
  white-space: nowrap;
}

.index-cell {
  white-space: nowrap;
}

.index-pill {
  display: inline-block;
  padding: 0.1rem 0.5rem;
  border-radius: 999px;
  font-size: 0.75rem;
  border: 1px solid #4a4a4a;
  color: #cfcfcf;
}

.index-ok { border-color: #2f6f3e; color: #9be3ad; }
.index-busy { border-color: #3d5a8a; color: #a9c4ff; }
.index-warn { border-color: #8a6d2f; color: #f3d48a; }
.index-error { border-color: #7f1d1d; color: #fca5a5; }

.index-progress {
  width: 5rem;
  height: 0.5rem;
  margin-left: 0.4rem;
  vertical-align: middle;
}

.index-actions {
  margin-left: 0.4rem;
}

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
  position: relative;
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
  /* Containing block for the absolutely positioned visually-hidden header
     text, so it scrolls with the table instead of widening the page. */
  position: relative;
  border: 1px solid #3a3a3a;
  border-radius: 8px;
  overflow: auto;
  background: #1a1a1a;
}

.empty-panel {
  padding: 0.85rem;
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

.badge.uploading,
.badge.downloading,
.badge.validating,
.badge.verifying,
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

.curated-panel {
  border: 1px solid #3a3a3a;
  border-radius: 8px;
  padding: 0.85rem;
  background: #1f1f1f;
}

.curated-panel-primary {
  margin-top: -0.1rem;
}

.curated-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.75rem;
  margin-bottom: 0.75rem;
}

.curated-header h3,
.curated-item h4 {
  margin: 0;
}

.curated-list {
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
}

.curated-item {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.75rem;
  border: 1px solid #343434;
  border-radius: 6px;
  background: #151515;
}

.curated-copy {
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
}

.curated-description {
  margin: 0;
  color: #cfcfcf;
  line-height: 1.45;
}

.curated-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
}

.curated-chip {
  border-radius: 999px;
  border: 1px solid #4d4d4d;
  padding: 0.2rem 0.5rem;
  font-size: 0.75rem;
  color: #d8d8d8;
}

.curated-actions {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
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

.loading-overlay {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.45);
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  z-index: 10;
}

.progress-bar-wrap {
  height: 4px;
  background: #2a2a2a;
  border-radius: 2px;
  overflow: hidden;
  margin: 3px 0 2px;
}

.progress-bar-fill {
  height: 100%;
  border-radius: 2px;
  transition: width 0.4s ease;
  background: #5cadc4;
}

.progress-bar-fill.completed {
  background: #90ee90;
}

.progress-bar-fill.failed {
  background: #ff6b6b;
}

.progress-bar-fill.cancelled {
  background: #b0b0b0;
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
  /* minmax(0, 1fr): a plain 1fr track never shrinks below its widest child
     (file table, URL/SHA-256 row), which made the page scroll sideways on
     phones. */
  .manager-layout {
    grid-template-columns: minmax(0, 1fr);
    min-height: auto;
  }

  .manager-main,
  .folder-sidebar {
    min-width: 0;
  }

  .folder-sidebar,
  .folder-sidebar.collapsed {
    width: 100%;
  }

  .download-create {
    grid-template-columns: minmax(0, 1fr);
  }

  .manager-panels {
    grid-template-columns: minmax(0, 1fr);
  }

  .curated-item {
    flex-direction: column;
  }
}
.download-resume {
  margin: 0.25rem 0;
  font-size: 0.85rem;
}

.sha256-input {
  font-family: monospace;
}
</style>
