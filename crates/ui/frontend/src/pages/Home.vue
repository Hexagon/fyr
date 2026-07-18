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
        <p class="small">PMTiles files available</p>
      </div>

      <div class="stat-card">
        <h3>📚 Books</h3>
        <p class="stat-number">{{ status.content_count?.books || 0 }}</p>
        <p class="small">EPUB & PDF files</p>
      </div>

      <div class="stat-card">
        <h3>📍 POIs</h3>
        <p class="stat-number">{{ status.content_count?.poi || 0 }}</p>
        <p class="small">Point collections</p>
      </div>

      <div class="stat-card">
        <h3>🤖 Models</h3>
        <p class="stat-number">{{ status.content_count?.models || 0 }}</p>
        <p class="small">GGUF files available</p>
      </div>

      <div class="stat-card">
        <h3>📦 Misc</h3>
        <p class="stat-number">{{ status.content_count?.misc || 0 }}</p>
        <p class="small">Generic local files</p>
      </div>
    </div>

    <div class="info-section">
      <h3>Data Directory</h3>
      <p v-if="status">Location: <code>{{ status.data_dir }}</code></p>
      <p class="info-text">
        All content is stored locally. No internet connection required after initial setup.
      </p>
    </div>

    <div v-if="storage" class="storage-section">
      <h3>📦 Storage Usage</h3>
      <div class="storage-info">
        <div class="total-storage">
          <p class="storage-label">Total Used:</p>
          <p class="storage-value">{{ storage.total_human }}</p>
          <p class="storage-detail">{{ storage.total_bytes.toLocaleString() }} bytes</p>
        </div>
        <div class="storage-breakdown">
          <div v-if="storage.by_category.maps" class="storage-item">
            <span class="storage-category">🗺️ Maps:</span>
            <span class="storage-amount">{{ storage.by_category.maps.human }}</span>
            <span class="storage-files">({{ storage.by_category.maps.files }} files)</span>
          </div>
          <div v-if="storage.by_category.books" class="storage-item">
            <span class="storage-category">📚 Books:</span>
            <span class="storage-amount">{{ storage.by_category.books.human }}</span>
            <span class="storage-files">({{ storage.by_category.books.files }} files)</span>
          </div>
          <div v-if="storage.by_category.poi" class="storage-item">
            <span class="storage-category">📍 POIs:</span>
            <span class="storage-amount">{{ storage.by_category.poi.human }}</span>
            <span class="storage-files">({{ storage.by_category.poi.files }} files)</span>
          </div>
          <div v-if="storage.by_category.models" class="storage-item">
            <span class="storage-category">🤖 Models:</span>
            <span class="storage-amount">{{ storage.by_category.models.human }}</span>
            <span class="storage-files">({{ storage.by_category.models.files }} files)</span>
          </div>
          <div v-if="storage.by_category.misc" class="storage-item">
            <span class="storage-category">📦 Misc:</span>
            <span class="storage-amount">{{ storage.by_category.misc.human }}</span>
            <span class="storage-files">({{ storage.by_category.misc.files }} files)</span>
          </div>
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

.info-section {
  background: #2a2a2a;
  padding: 1.5rem;
  border-radius: 8px;
  border-left: 4px solid #667eea;
  box-shadow: 0 2px 8px rgba(0,0,0,0.3);
}

.info-section h3 {
  color: #e0e0e0;
}

.info-section p {
  color: #b0b0b0;
}

.info-section code {
  background: #1a1a1a;
  padding: 0.25rem 0.5rem;
  border-radius: 3px;
  font-family: monospace;
  color: #90ee90;
  border: 1px solid #3a3a3a;
}

.info-text {
  margin-top: 0.5rem;
  color: #b0b0b0;
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
  padding: 1.5rem;
  border-radius: 8px;
  border-left: 4px solid #667eea;
  box-shadow: 0 2px 8px rgba(0,0,0,0.3);
}

.storage-section h3 {
  color: #e0e0e0;
  margin-bottom: 1.5rem;
}

.storage-info {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1.5rem;
}

.total-storage {
  background: #1a1a1a;
  padding: 1rem;
  border-radius: 6px;
  border: 1px solid #3a3a3a;
}

.storage-label {
  color: #808080;
  font-size: 0.9rem;
  margin: 0 0 0.5rem 0;
}

.storage-value {
  font-size: 1.8rem;
  font-weight: bold;
  color: #667eea;
  margin: 0;
}

.storage-detail {
  color: #808080;
  font-size: 0.85rem;
  margin: 0.5rem 0 0 0;
}

.storage-breakdown {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.storage-item {
  display: flex;
  align-items: center;
  gap: 1rem;
  background: #1a1a1a;
  padding: 0.75rem;
  border-radius: 6px;
  border: 1px solid #3a3a3a;
}

.storage-category {
  color: #b0b0b0;
  font-weight: 500;
  min-width: 80px;
}

.storage-amount {
  color: #667eea;
  font-weight: 600;
  margin-left: auto;
}

.storage-files {
  color: #808080;
  font-size: 0.85rem;
  min-width: 100px;
  text-align: right;
}
</style>
