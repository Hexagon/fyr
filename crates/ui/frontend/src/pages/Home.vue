<template>
  <div class="home-page">
    <AlertBanner />
    <section v-if="continueReading.length" class="continue-section" aria-labelledby="continue-reading-heading">
      <h3 id="continue-reading-heading">📖 {{ t('home.continueReading') }}</h3>
      <ul class="continue-list">
        <li v-for="item in continueReading" :key="item.filename">
          <router-link
            :to="item.href"
            class="continue-card"
            :aria-label="t('home.resumeBook', { title: item.title })"
          >
            <span class="continue-title-row">
              <span class="continue-title">{{ item.title }}</span>
              <span class="continue-format">{{ item.format }}</span>
            </span>
            <span v-if="item.positionLabel" class="continue-position">{{ item.positionLabel }}</span>
            <span class="continue-meta">
              {{ formatRelativeTime(item.lastOpened) }}<template v-if="item.bookmarkCount"> · {{ t('home.bookmarkCount', { count: item.bookmarkCount }, item.bookmarkCount) }}</template>
            </span>
          </router-link>
        </li>
      </ul>
    </section>

    <div class="stats-grid" v-if="status">
      <div class="stat-card">
        <h3>📊 {{ t('home.systemStatus') }}</h3>
        <p class="status-badge" :class="status.status">{{ status.status }}</p>
        <p class="small">{{ t('home.version', { version: status.version }) }}</p>
        <dl class="status-details">
          <div>
            <dt>{{ t('home.timeSource') }}</dt>
            <dd>{{ timeSourceText }}</dd>
          </div>
          <div>
            <dt>{{ t('home.clock') }}</dt>
            <dd>{{ clock.timeText }}</dd>
          </div>
          <div>
            <dt>{{ t('home.date') }}</dt>
            <dd>{{ clock.dateText }}</dd>
          </div>
          <div>
            <dt>{{ t('home.location') }}</dt>
            <dd>{{ locationLabel }}</dd>
          </div>
          <div v-if="locationState.location">
            <dt>{{ t('home.coordinates') }}</dt>
            <dd>{{ locationCoordinates }}</dd>
          </div>
          <div>
            <dt>{{ t('home.sun') }}</dt>
            <dd>{{ sunSummary }}</dd>
          </div>
        </dl>
      </div>

      <div class="stat-card">
        <h3>🗺️ {{ t('home.maps') }}</h3>
        <p class="stat-number">{{ status.content_count?.maps || 0 }}</p>
        <p class="stat-meta">{{ categoryStorageSummary('maps') }}</p>
        <p class="small">{{ t('home.mapsHint') }}</p>
      </div>

      <div class="stat-card">
        <h3>📚 {{ t('home.books') }}</h3>
        <p class="stat-number">{{ status.content_count?.books || 0 }}</p>
        <p class="stat-meta">{{ categoryStorageSummary('books') }}</p>
        <p class="small">{{ t('home.booksHint') }}</p>
      </div>

      <div class="stat-card">
        <h3>📍 {{ t('home.poi') }}</h3>
        <p class="stat-number">{{ status.content_count?.poi || 0 }}</p>
        <p class="stat-meta">{{ categoryStorageSummary('poi') }}</p>
        <p class="small">{{ t('home.poiHint') }}</p>
      </div>

      <div class="stat-card">
        <h3>🤖 {{ t('home.models') }}</h3>
        <p class="stat-number">{{ status.content_count?.models || 0 }}</p>
        <p class="stat-meta">{{ categoryStorageSummary('models') }}</p>
        <p class="small">{{ t('home.modelsHint') }}</p>
      </div>

      <div class="stat-card">
        <h3>📦 {{ t('home.misc') }}</h3>
        <p class="stat-number">{{ status.content_count?.misc || 0 }}</p>
        <p class="stat-meta">{{ categoryStorageSummary('misc') }}</p>
        <p class="small">{{ t('home.miscHint') }}</p>
      </div>
    </div>

    <div v-if="storage && !adminLocked" class="storage-section">
      <h3>📦 {{ t('home.storageUsage') }}</h3>
      <p class="storage-path">
        {{ t('home.dataDirectory') }} <code>{{ storage.data_dir }}</code>
      </p>
      <p v-if="status?.tls?.fingerprint_sha256" class="storage-path tls-fingerprint">
        {{ t('home.tlsCertificate', { mode: status.tls.mode }) }}
        <code>{{ status.tls.fingerprint_sha256 }}</code>
      </p>
      <div class="storage-info compact">
        <div class="total-storage compact-item">
          <p class="storage-label">{{ t('home.used') }}</p>
          <p class="storage-value">{{ storage.total_human }}</p>
          <p class="storage-detail">{{ t('home.bytes', { n: n(storage.total_bytes) }) }}</p>
        </div>
        <div class="total-storage compact-item">
          <p class="storage-label">{{ t('home.free') }}</p>
          <p class="storage-value">{{ freeSpaceLabel }}</p>
          <p class="storage-detail" v-if="storage.free_bytes !== null && storage.free_bytes !== undefined">{{ t('home.bytes', { n: n(storage.free_bytes) }) }}</p>
        </div>
        <div class="total-storage compact-item" v-if="storage.capacity_human">
          <p class="storage-label">{{ t('home.capacity') }}</p>
          <p class="storage-value">{{ storage.capacity_human }}</p>
          <p class="storage-detail" v-if="storage.capacity_bytes !== null && storage.capacity_bytes !== undefined">{{ t('home.bytes', { n: n(storage.capacity_bytes) }) }}</p>
        </div>
      </div>
      <router-link to="/content" class="storage-manage-link">{{ t('home.manageContent') }} &rarr;</router-link>
    </div>

    <div v-if="loading" class="loading" role="status" aria-live="polite" aria-busy="true">
      <span class="spinner" aria-hidden="true"></span>
      <p>{{ t('home.loading') }}</p>
    </div>
    <div v-if="error" class="error-message" role="alert" aria-live="assertive">
      <p>⚠️ {{ error }}</p>
      <p class="small">{{ t('home.errorHint') }}</p>
    </div>
  </div>
</template>

<script setup>
import { computed, ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { apiService } from '../services/api'
import { useLocationState } from '../services/location'
import { getLocationClock } from '../services/locationClock'
import { isAdminLocked } from '../services/auth'
import { progressStore } from '../modules/library/progress'
import { formatRelativeTime, selectContinueReading } from '../modules/library/continueReading'
import AlertBanner from '../modules/messaging/AlertBanner.vue'

const { t, n, locale } = useI18n()
const status = ref(null)
const storage = ref(null)
const loading = ref(true)
const error = ref(null)
const locationState = useLocationState()

const adminLocked = computed(() => isAdminLocked())

const clock = computed(() => getLocationClock(new Date(), locationState.location, locale.value))
const timeSourceText = computed(() => clock.value.mode === 'location' ? t('home.savedLocation') : t('home.systemTime'))
const locationLabel = computed(() => locationState.location?.label?.trim() || t('home.noSavedLocation'))
const locationCoordinates = computed(() => {
  if (!locationState.location) return t('common.unavailable')

  return `${locationState.location.latitude.toFixed(4)}, ${locationState.location.longitude.toFixed(4)}`
})
const sunSummary = computed(() => {
  if (!clock.value.sunriseText && !clock.value.sunsetText) {
    return t('common.unavailable')
  }

  const parts = []
  if (clock.value.sunriseText) parts.push(`↑ ${clock.value.sunriseText}`)
  if (clock.value.sunsetText) parts.push(`↓ ${clock.value.sunsetText}`)
  return parts.join(' · ')
})
const freeSpaceLabel = computed(() => storage.value?.free_human || t('common.unavailable'))

const categoryStorageSummary = (category) => {
  const categoryInfo = storage.value?.by_category?.[category]
  if (!categoryInfo) {
    return t('home.storagePending')
  }

  const fileCount = typeof categoryInfo.files === 'number' ? categoryInfo.files : 0
  return `${categoryInfo.human} • ${t('common.fileCount', { count: fileCount }, fileCount)}`
}

// Continue reading: recent books from this browser's progress store, limited
// to files still present in the library listing.
const continueReading = ref([])

const loadContinueReading = async () => {
  const recent = progressStore.listRecent(30)
  if (!recent.length) return
  try {
    const response = await apiService.getBooks()
    continueReading.value = selectContinueReading(recent, response.data || [], 6)
  } catch {
    continueReading.value = []
  }
}

onMounted(async () => {
  loadContinueReading()
  try {
    const response = await apiService.getStatus()
    status.value = response.data
    
    const storageResponse = await apiService.getStorage()
    storage.value = storageResponse.data
  } catch (err) {
    error.value = apiService.handleError(err)
  } finally {
    loading.value = false
  }
})
</script>

<style scoped>
.home-page {
  display: flex;
  flex-direction: column;
  gap: 2rem;
}

.continue-section {
  background: #2a2a2a;
  padding: 1rem;
  border-radius: 8px;
  border-left: 4px solid #40c0b5;
  box-shadow: 0 2px 8px rgba(0,0,0,0.3);
}

.continue-section h3 {
  color: #e0e0e0;
  margin: 0 0 0.75rem;
  font-size: 1rem;
}

.continue-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 0.75rem;
}

.continue-card {
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
  height: 100%;
  background: #1a1a1a;
  border: 1px solid #3a3a3a;
  border-radius: 6px;
  padding: 0.7rem 0.8rem;
  color: #e0e0e0;
  text-decoration: none;
  transition: border-color 0.2s ease;
}

.continue-card:hover,
.continue-card:focus-visible {
  border-color: #40c0b5;
}

.continue-card:focus-visible {
  outline: 2px solid #40c0b5;
  outline-offset: 2px;
}

.continue-title-row {
  display: flex;
  gap: 0.4rem;
  align-items: center;
}

.continue-title {
  flex: 1;
  min-width: 0;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.continue-format {
  background: rgba(64, 192, 181, 0.2);
  border: 1px solid rgba(64, 192, 181, 0.5);
  color: #b9fff6;
  border-radius: 999px;
  padding: 0.05rem 0.4rem;
  font-size: 0.68rem;
}

.continue-position {
  color: #b0b0b0;
  font-size: 0.8rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.continue-meta {
  color: #808080;
  font-size: 0.75rem;
}

.stats-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 1rem;
}

.stat-card {
  background: #2a2a2a;
  padding: 1.5rem;
  border-radius: 8px;
  box-shadow: 0 2px 8px rgba(0,0,0,0.3);
  border-left: 4px solid #667eea;
}

.stat-card h3 {
  font-size: 0.9rem;
  color: #b0b0b0;
  margin-bottom: 0.5rem;
}

.stat-number {
  font-size: 2rem;
  font-weight: bold;
  color: #667eea;
  margin: 0.5rem 0;
}

.stat-meta {
  color: #8db2ff;
  font-size: 0.88rem;
  margin: 0.2rem 0 0.35rem;
}

.small {
  font-size: 0.85rem;
  color: #999;
  margin: 0.35rem 0 0;
}

.status-details {
  display: grid;
  gap: 0.55rem;
  margin: 1rem 0 0;
}

.status-details div {
  display: flex;
  justify-content: space-between;
  gap: 0.75rem;
  padding-top: 0.45rem;
  border-top: 1px solid rgba(255, 255, 255, 0.08);
}

.status-details dt,
.status-details dd {
  margin: 0;
  font-size: 0.84rem;
}

.status-details dt {
  color: #8d8d8d;
}

.status-details dd {
  color: #d9d9d9;
  text-align: right;
}

.status-badge {
  display: inline-block;
  padding: 0.25rem 0.75rem;
  border-radius: 20px;
  font-size: 0.9rem;
  font-weight: 600;
}

.status-badge.ok {
  background: #2d5a2d;
  color: #90ee90;
}

.loading, .error-message {
  padding: 2rem;
  text-align: center;
  background: #2a2a2a;
  border-radius: 8px;
  color: #b0b0b0;
}

.spinner {
  display: inline-block;
  width: 1.3rem;
  height: 1.3rem;
  border: 2px solid rgba(255, 255, 255, 0.25);
  border-top-color: #8db2ff;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.error-message {
  background: #3d2a2a;
  color: #ff6b6b;
  border-left: 4px solid #ff6b6b;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.storage-section {
  background: #2a2a2a;
  padding: 1rem;
  border-radius: 8px;
  border-left: 4px solid #667eea;
  box-shadow: 0 2px 8px rgba(0,0,0,0.3);
}

.storage-section h3 {
  color: #e0e0e0;
  margin-bottom: 0.75rem;
  font-size: 1rem;
}

.storage-path {
  margin: 0 0 0.75rem;
  color: #a5a5a5;
  font-size: 0.82rem;
}

.storage-path code {
  background: #1a1a1a;
  padding: 0.2rem 0.35rem;
  border-radius: 3px;
  font-family: monospace;
  color: #90ee90;
  border: 1px solid #3a3a3a;
}

.storage-info {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
  gap: 0.75rem;
}

.total-storage {
  background: #1a1a1a;
  padding: 0.75rem;
  border-radius: 6px;
  border: 1px solid #3a3a3a;
}

.compact-item {
  min-height: 0;
}

.storage-label {
  color: #808080;
  font-size: 0.8rem;
  margin: 0 0 0.35rem 0;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.storage-value {
  font-size: 1.15rem;
  font-weight: bold;
  color: #667eea;
  margin: 0;
}

.storage-detail {
  color: #808080;
  font-size: 0.75rem;
  margin: 0.3rem 0 0 0;
}

.storage-manage-link {
  display: inline-block;
  margin-top: 0.75rem;
  color: #8db2ff;
  text-decoration: none;
  font-size: 0.85rem;
  opacity: 0.85;
  transition: opacity 0.15s;
}

.storage-manage-link:hover {
  opacity: 1;
  text-decoration: underline;
}
.tls-fingerprint code {
  word-break: break-all;
}
</style>
