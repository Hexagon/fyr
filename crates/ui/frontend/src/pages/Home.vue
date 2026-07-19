<template>
  <div class="home-page">
    <div class="stats-grid" v-if="status">
      <div class="stat-card">
        <h3>📊 System Status</h3>
        <p class="status-badge" :class="status.status">{{ status.status }}</p>
        <p class="small">Version: {{ status.version }}</p>
        <dl class="status-details">
          <div>
            <dt>Time source</dt>
            <dd>{{ timeSourceText }}</dd>
          </div>
          <div>
            <dt>Clock</dt>
            <dd>{{ clock.timeText }}</dd>
          </div>
          <div>
            <dt>Date</dt>
            <dd>{{ clock.dateText }}</dd>
          </div>
          <div>
            <dt>Location</dt>
            <dd>{{ locationLabel }}</dd>
          </div>
          <div v-if="locationState.location">
            <dt>Coordinates</dt>
            <dd>{{ locationCoordinates }}</dd>
          </div>
          <div>
            <dt>Sun</dt>
            <dd>{{ sunSummary }}</dd>
          </div>
          <div>
            <dt>Mode</dt>
            <dd>Offline active</dd>
          </div>
        </dl>
      </div>

      <div class="stat-card">
        <h3>🗺️ Maps</h3>
        <p class="stat-number">{{ status.content_count?.maps || 0 }}</p>
        <p class="stat-meta">{{ categoryStorageSummary('maps') }}</p>
        <p class="small">PMTiles files available</p>
      </div>

      <div class="stat-card">
        <h3>📚 Books</h3>
        <p class="stat-number">{{ status.content_count?.books || 0 }}</p>
        <p class="stat-meta">{{ categoryStorageSummary('books') }}</p>
        <p class="small">EPUB, PDF, MOBI, Markdown, and ZIM</p>
      </div>

      <div class="stat-card">
        <h3>📍 POIs</h3>
        <p class="stat-number">{{ status.content_count?.poi || 0 }}</p>
        <p class="stat-meta">{{ categoryStorageSummary('poi') }}</p>
        <p class="small">GeoJSON, JSON, and FlatGeoBuf</p>
      </div>

      <div class="stat-card">
        <h3>🤖 Models</h3>
        <p class="stat-number">{{ status.content_count?.models || 0 }}</p>
        <p class="stat-meta">{{ categoryStorageSummary('models') }}</p>
        <p class="small">GGUF files available</p>
      </div>

      <div class="stat-card">
        <h3>📦 Misc</h3>
        <p class="stat-number">{{ status.content_count?.misc || 0 }}</p>
        <p class="stat-meta">{{ categoryStorageSummary('misc') }}</p>
        <p class="small">General files and installers (TXT, CSV, ZIP, 7Z, EXE, MSI)</p>
      </div>
    </div>

    <div v-if="storage" class="storage-section">
      <h3>📦 Storage Usage</h3>
      <p class="storage-path">
        Data Directory: <code>{{ storage.data_dir }}</code>
      </p>
      <div class="storage-info compact">
        <div class="total-storage compact-item">
          <p class="storage-label">Used</p>
          <p class="storage-value">{{ storage.total_human }}</p>
          <p class="storage-detail">{{ storage.total_bytes.toLocaleString() }} bytes</p>
        </div>
        <div class="total-storage compact-item">
          <p class="storage-label">Free</p>
          <p class="storage-value">{{ freeSpaceLabel }}</p>
          <p class="storage-detail" v-if="storage.free_bytes !== null && storage.free_bytes !== undefined">{{ storage.free_bytes.toLocaleString() }} bytes</p>
        </div>
        <div class="total-storage compact-item" v-if="storage.capacity_human">
          <p class="storage-label">Capacity</p>
          <p class="storage-value">{{ storage.capacity_human }}</p>
          <p class="storage-detail" v-if="storage.capacity_bytes !== null && storage.capacity_bytes !== undefined">{{ storage.capacity_bytes.toLocaleString() }} bytes</p>
        </div>
      </div>
    </div>

    <div v-if="loading" class="loading">
      <p>Loading server status...</p>
    </div>
    <div v-if="error" class="error-message">
      <p>⚠️ {{ error }}</p>
    </div>
  </div>
</template>

<script setup>
import { computed, ref, onMounted } from 'vue'
import { apiService } from '../services/api'
import { useLocationState } from '../services/location'
import { getLocationClock } from '../services/locationClock'

const status = ref(null)
const storage = ref(null)
const loading = ref(true)
const error = ref(null)
const locationState = useLocationState()

const clock = computed(() => getLocationClock(new Date(), locationState.location))
const timeSourceText = computed(() => clock.value.mode === 'location' ? 'Saved location' : 'System time')
const locationLabel = computed(() => locationState.location?.label?.trim() || 'No saved location')
const locationCoordinates = computed(() => {
  if (!locationState.location) return 'Unavailable'

  return `${locationState.location.latitude.toFixed(4)}, ${locationState.location.longitude.toFixed(4)}`
})
const sunSummary = computed(() => {
  if (!clock.value.sunriseText && !clock.value.sunsetText) {
    return 'Unavailable'
  }

  const parts = []
  if (clock.value.sunriseText) parts.push(`↑ ${clock.value.sunriseText}`)
  if (clock.value.sunsetText) parts.push(`↓ ${clock.value.sunsetText}`)
  return parts.join(' · ')
})
const freeSpaceLabel = computed(() => storage.value?.free_human || 'Unavailable')

const categoryStorageSummary = (category) => {
  const categoryInfo = storage.value?.by_category?.[category]
  if (!categoryInfo) {
    return 'Storage pending'
  }

  const fileCount = typeof categoryInfo.files === 'number' ? categoryInfo.files : 0
  const fileLabel = fileCount === 1 ? 'file' : 'files'
  return `${categoryInfo.human} • ${fileCount} ${fileLabel}`
}

onMounted(async () => {
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

.error-message {
  background: #3d2a2a;
  color: #ff6b6b;
  border-left: 4px solid #ff6b6b;
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
</style>
