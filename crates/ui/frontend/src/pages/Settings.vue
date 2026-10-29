<template>
  <div class="settings-page">
    <section class="panel" aria-labelledby="language-heading">
      <div class="panel-header">
        <h3 id="language-heading">{{ t('settings.language.title') }}</h3>
      </div>
      <p class="body-copy">{{ t('settings.language.hint') }}</p>
      <LanguageSwitcher class="settings-language" />
    </section>

    <section class="panel">
      <div class="panel-header">
        <h3>{{ t('settings.location.title') }}</h3>
        <span class="badge" :class="settingsState.settings.location ? 'active' : 'inactive'">
          {{ settingsState.settings.location ? t('settings.location.saved') : t('home.systemTime') }}
        </span>
      </div>

      <div class="location-summary" v-if="settingsState.settings.location">
        <p><strong>{{ t('settings.location.latitudeLabel') }}</strong> {{ settingsState.settings.location.latitude.toFixed(6) }}</p>
        <p><strong>{{ t('settings.location.longitudeLabel') }}</strong> {{ settingsState.settings.location.longitude.toFixed(6) }}</p>
        <p v-if="settingsState.settings.location.label"><strong>{{ t('settings.location.labelLabel') }}</strong> {{ settingsState.settings.location.label }}</p>
      </div>

      <form class="settings-form" @submit.prevent="handleSave">
        <label>
          {{ t('settings.location.label') }}
          <input v-model="form.label" type="text" :placeholder="t('settings.location.labelPlaceholder')" />
        </label>

        <div class="grid-2">
          <label>
            {{ t('settings.location.latitude') }}
            <input v-model="form.latitude" type="number" step="0.000001" placeholder="59.3293" required />
          </label>

          <label>
            {{ t('settings.location.longitude') }}
            <input v-model="form.longitude" type="number" step="0.000001" placeholder="18.0686" required />
          </label>
        </div>

        <div class="actions">
          <button type="submit" class="btn btn-primary" :disabled="saving">
            {{ saving ? t('common.saving') : t('settings.location.save') }}
          </button>
          <button type="button" class="btn btn-secondary" @click="handleBrowserLocation" :disabled="saving || !browserLocationAvailable">
            {{ t('settings.location.useBrowser') }}
          </button>
          <button type="button" class="btn btn-secondary" @click="handleClear" :disabled="saving || !settingsState.settings.location">
            {{ t('settings.location.clear') }}
          </button>
        </div>
        <p v-if="browserGeoNote" class="geo-note">{{ browserGeoNote }}</p>
      </form>

      <p v-if="message" class="status-message">{{ message }}</p>
      <p v-if="error" class="error-message">{{ error }}</p>
    </section>

    <section class="panel">
      <h3>{{ t('settings.bus.title') }}</h3>
      <p class="body-copy">{{ t('settings.bus.body') }}</p>
    </section>

    <section class="panel">
      <div class="panel-header">
        <h3>{{ t('settings.modules.title') }}</h3>
        <span class="badge inactive">{{ t('settings.modules.count', { count: moduleCount }, moduleCount) }}</span>
      </div>

      <div class="module-editor">
        <h4>{{ t('settings.downloads.title') }}</h4>
        <p class="body-copy">{{ t('settings.downloads.body') }}</p>
        <div class="grid-2">
          <label>
            {{ t('settings.downloads.timeout') }}
            <input
              v-model="downloadTimeoutSeconds"
              type="number"
              min="30"
              max="86400"
              step="1"
              placeholder="300"
            />
          </label>
        </div>
        <div class="actions">
          <button type="button" class="btn btn-primary" @click="handleSaveDownloadTimeout" :disabled="saving">
            {{ t('settings.downloads.save') }}
          </button>
        </div>
      </div>

      <div v-if="moduleEntries.length" class="module-list">
        <article v-for="entry in moduleEntries" :key="entry.name" class="module-item">
          <div class="module-item-header">
            <strong>{{ entry.name }}</strong>
            <button type="button" class="link-button" @click="removeModule(entry.name)">{{ t('common.remove') }}</button>
          </div>
          <pre>{{ entry.value }}</pre>
        </article>
      </div>
      <p v-else class="body-copy">{{ t('settings.modules.empty') }}</p>

      <div class="module-editor">
        <h4>{{ t('settings.modules.addTitle') }}</h4>
        <div class="grid-2">
          <label>
            {{ t('settings.modules.name') }}
            <input v-model="moduleForm.name" type="text" placeholder="location" />
          </label>

          <label>
            {{ t('settings.modules.payload') }}
            <input v-model="moduleForm.payload" type="text" placeholder='{"enabled": true}' />
          </label>
        </div>
        <div class="actions">
          <button type="button" class="btn btn-primary" @click="handleSaveModuleState" :disabled="saving || !moduleForm.name">
            {{ t('settings.modules.save') }}
          </button>
        </div>
      </div>
    </section>

    <section v-if="!adminLocked" class="panel" aria-labelledby="backup-heading">
      <div class="panel-header">
        <h3 id="backup-heading">{{ t('settings.backup.title') }}</h3>
        <span class="badge inactive">{{ t('app.admin') }}</span>
      </div>
      <p class="body-copy">{{ t('settings.backup.body') }}</p>
      <div class="actions">
        <button type="button" class="btn btn-primary" :disabled="backup.busy" @click="handleDownloadBackup">
          {{ backup.busy === 'download' ? t('settings.backup.preparing') : t('settings.backup.download') }}
        </button>
      </div>

      <div class="module-editor">
        <h4>{{ t('settings.backup.restoreTitle') }}</h4>
        <p class="body-copy">{{ t('settings.backup.restoreBody') }}</p>
        <label>
          {{ t('settings.backup.file') }}
          <input ref="restoreInput" type="file" accept=".zip,application/zip" :disabled="Boolean(backup.busy)" @change="onRestoreFile" />
        </label>
        <div class="actions">
          <button type="button" class="btn btn-secondary" :disabled="!backup.file || Boolean(backup.busy)" @click="handleRestore">
            {{ backup.busy === 'restore' ? t('settings.backup.restoring', { pct: backup.progress }) : t('settings.backup.restore') }}
          </button>
        </div>
      </div>

      <div v-if="backup.result" class="restore-result" role="status">
        <p class="status-message">{{ backup.result.message }}</p>
        <p class="body-copy">
          {{ t('settings.backup.from', { date: backup.result.backup_created_at || t('settings.backup.unknownDate'), version: backup.result.backup_fyr_version || '?' }) }}
        </p>
        <p v-if="backup.result.restored?.length" class="body-copy">{{ t('settings.backup.restoredNow', { items: backup.result.restored.join(', ') }) }}</p>
        <p v-if="backup.result.restart_required" class="restart-note">
          <i18n-t keypath="settings.backup.restart" scope="global">
            <template #command><code>docker restart fyr</code></template>
            <template #items>{{ backup.result.pending_restart.join(t('settings.backup.and')) }}</template>
          </i18n-t>
        </p>
      </div>
      <p v-if="backup.error" class="error-message" role="alert">{{ backup.error }}</p>
    </section>
  </div>
</template>

<script setup>
import { computed, onMounted, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import LanguageSwitcher from '../components/LanguageSwitcher.vue'
import { apiService } from '../services/api'
import { clearModuleState, clearSavedLocation, loadAppSettings, saveLocation, updateModuleState, useSettingsState } from '../services/settings'
import { isAdminLocked } from '../services/auth'
import { saveBlob } from '../services/fileTransfer'

const { t } = useI18n()
const settingsState = useSettingsState()
const saving = ref(false)
const message = ref('')
const error = ref('')

const form = reactive({
  label: '',
  latitude: '',
  longitude: ''
})

const moduleForm = reactive({
  name: 'location',
  payload: '{"source":"settings-page","updatedAt":""}'
})
const downloadTimeoutSeconds = ref('300')

// Backup & restore (admin only; the server enforces this independently).
const adminLocked = computed(() => isAdminLocked())
const restoreInput = ref(null)
const backup = reactive({ busy: '', progress: 0, file: null, result: null, error: '' })

const handleDownloadBackup = async () => {
  backup.error = ''
  backup.busy = 'download'
  try {
    const { blob, filename } = await apiService.downloadBackup()
    saveBlob(blob, filename)
  } catch (downloadError) {
    backup.error = downloadError?.message || apiService.handleError(downloadError)
  } finally {
    backup.busy = ''
  }
}

const onRestoreFile = (event) => {
  backup.file = event.target.files?.[0] || null
  backup.result = null
  backup.error = ''
}

const handleRestore = async () => {
  const file = backup.file
  if (!file) return
  const confirmed = window.confirm(
    t('settings.backup.confirm', { name: file.name })
  )
  if (!confirmed) return
  backup.error = ''
  backup.result = null
  backup.progress = 0
  backup.busy = 'restore'
  try {
    const response = await apiService.restoreBackup(file, (loaded, total) => {
      backup.progress = total ? Math.round((loaded / total) * 100) : 0
    })
    backup.result = response.data
    backup.file = null
    if (restoreInput.value) restoreInput.value.value = ''
    await loadAppSettings()
    hydrateForm()
    hydrateDownloadTimeout()
  } catch (restoreError) {
    backup.error = apiService.handleError(restoreError)
  } finally {
    backup.busy = ''
  }
}

const browserGeoNote = computed(() => {
  if (typeof navigator === 'undefined' || !('geolocation' in navigator)) {
    return t('settings.geo.unsupported')
  }
  if (typeof window !== 'undefined' && !window.isSecureContext) {
    return t('settings.geo.insecure')
  }
  return ''
})
const browserLocationAvailable = computed(() => browserGeoNote.value === '')
const moduleEntries = computed(() => Object.entries(settingsState.settings.modules || {}).map(([name, value]) => ({ name, value: JSON.stringify(value, null, 2) })))
const moduleCount = computed(() => moduleEntries.value.length)

const hydrateDownloadTimeout = () => {
  const value = settingsState.settings.modules?.downloads?.request_timeout_seconds
  const fallback = Number.isFinite(Number(value)) && Number(value) > 0 ? Number(value) : 300
  downloadTimeoutSeconds.value = String(Math.trunc(fallback))
}

const hydrateForm = () => {
  const location = settingsState.settings.location
  form.label = location?.label || ''
  form.latitude = location?.latitude ?? ''
  form.longitude = location?.longitude ?? ''
}

onMounted(async () => {
  if (!settingsState.loaded) {
    try {
      await loadAppSettings()
    } catch (loadError) {
      error.value = apiService.handleError(loadError)
    }
  }

  hydrateForm()
  hydrateDownloadTimeout()
})

const handleSave = async () => {
  message.value = ''
  error.value = ''
  saving.value = true

  const latitude = Number.parseFloat(form.latitude)
  const longitude = Number.parseFloat(form.longitude)

  if (Number.isNaN(latitude) || Number.isNaN(longitude)) {
    error.value = t('settings.location.invalid')
    saving.value = false
    return
  }

  try {
    await saveLocation({
      latitude,
      longitude,
      label: form.label.trim() || null
    })
    message.value = t('settings.location.savedMessage')
  } catch (saveError) {
    error.value = apiService.handleError(saveError)
  } finally {
    saving.value = false
  }
}

const handleClear = async () => {
  message.value = ''
  error.value = ''
  saving.value = true

  try {
    await clearSavedLocation()
    hydrateForm()
    message.value = t('settings.location.cleared')
  } catch (clearError) {
    error.value = apiService.handleError(clearError)
  } finally {
    saving.value = false
  }
}

const handleBrowserLocation = async () => {
  message.value = ''
  error.value = ''
  saving.value = true

  if (!browserLocationAvailable.value) {
    error.value = t('settings.geo.unavailable')
    saving.value = false
    return
  }

  navigator.geolocation.getCurrentPosition(
    async (position) => {
      try {
        await saveLocation({
          latitude: position.coords.latitude,
          longitude: position.coords.longitude,
          label: form.label.trim() || t('settings.location.browserLabel')
        })
        hydrateForm()
        message.value = t('settings.location.captured')
      } catch (saveError) {
        error.value = apiService.handleError(saveError)
      } finally {
        saving.value = false
      }
    },
    (geoError) => {
      error.value = geoError.message || t('settings.geo.readFailed')
      saving.value = false
    },
    {
      enableHighAccuracy: true,
      timeout: 10000,
      maximumAge: 60000
    }
  )
}

const removeModule = async (moduleName) => {
  message.value = ''
  error.value = ''
  saving.value = true

  try {
    await clearModuleState(moduleName)
    message.value = t('settings.modules.removed', { name: moduleName })
  } catch (moduleError) {
    error.value = apiService.handleError(moduleError)
  } finally {
    saving.value = false
  }
}

const handleSaveModuleState = async () => {
  message.value = ''
  error.value = ''
  saving.value = true

  try {
    const parsedPayload = moduleForm.payload.trim() ? JSON.parse(moduleForm.payload) : {}
    await updateModuleState(moduleForm.name.trim(), parsedPayload)
    message.value = t('settings.modules.updated', { name: moduleForm.name })
  } catch (moduleError) {
    error.value = moduleError instanceof SyntaxError
      ? t('settings.modules.invalidJson')
      : apiService.handleError(moduleError)
  } finally {
    saving.value = false
  }
}

const handleSaveDownloadTimeout = async () => {
  message.value = ''
  error.value = ''
  saving.value = true

  try {
    const parsed = Number.parseInt(String(downloadTimeoutSeconds.value || '').trim(), 10)
    if (!Number.isFinite(parsed)) {
      throw new Error(t('settings.downloads.invalid'))
    }

    const bounded = Math.min(86400, Math.max(30, parsed))
    await updateModuleState('downloads', {
      ...(settingsState.settings.modules?.downloads || {}),
      request_timeout_seconds: bounded
    })
    downloadTimeoutSeconds.value = String(bounded)
    message.value = t('settings.downloads.saved', { seconds: bounded })
  } catch (downloadSettingsError) {
    error.value = apiService.handleError(downloadSettingsError)
  } finally {
    saving.value = false
  }
}
</script>

<style scoped>
.settings-page {
  display: grid;
  gap: 1rem;
}

.panel {
  background: #22242b;
  border: 1px solid #343844;
  border-radius: 14px;
  padding: 1.25rem;
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.2);
}

.hero {
  background: linear-gradient(135deg, rgba(102, 126, 234, 0.2), rgba(118, 75, 162, 0.12));
}

.eyebrow {
  margin: 0 0 0.35rem;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.75rem;
  color: #9ea7ff;
}

.lead,
.body-copy {
  margin: 0.5rem 0 0;
  color: #b8bfd0;
}

.module-list {
  display: grid;
  gap: 0.75rem;
}

.module-item {
  padding: 0.9rem;
  border-radius: 12px;
  background: rgba(18, 20, 26, 0.6);
  border: 1px solid #323745;
}

.module-item-header {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  align-items: center;
  margin-bottom: 0.5rem;
}

.module-item pre {
  margin: 0;
  white-space: pre-wrap;
  color: #d9def2;
  font-size: 0.85rem;
}

.module-editor {
  margin-top: 1rem;
  padding-top: 1rem;
  border-top: 1px solid #343844;
  display: grid;
  gap: 0.75rem;
}

.module-editor h4 {
  margin: 0;
  color: #e9ecf7;
}

.link-button {
  border: 0;
  background: transparent;
  color: #9ea7ff;
  cursor: pointer;
  padding: 0;
}

.panel-header {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  align-items: center;
  margin-bottom: 1rem;
}

.badge {
  padding: 0.35rem 0.65rem;
  border-radius: 999px;
  font-size: 0.75rem;
  font-weight: 700;
}

.badge.active {
  background: rgba(45, 90, 45, 0.75);
  color: #b9f7b9;
}

.badge.inactive {
  background: rgba(90, 78, 45, 0.75);
  color: #f4de9a;
}

.location-summary {
  display: grid;
  gap: 0.25rem;
  margin-bottom: 1rem;
  color: #d7dbe8;
}

.settings-form {
  display: grid;
  gap: 1rem;
}

label {
  display: grid;
  gap: 0.45rem;
  color: #d7dbe8;
}

input {
  width: 100%;
  border: 1px solid #3a3f4d;
  border-radius: 10px;
  background: #171a20;
  color: #edf0f7;
  padding: 0.8rem 0.9rem;
}

input:focus {
  outline: 2px solid rgba(102, 126, 234, 0.55);
  outline-offset: 1px;
}

.grid-2 {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 1rem;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
}

.btn {
  border: 0;
  border-radius: 10px;
  padding: 0.8rem 1rem;
  font-weight: 700;
  cursor: pointer;
}

.btn-primary {
  background: linear-gradient(135deg, #667eea, #7b5cff);
  color: white;
}

.btn-secondary {
  background: #2f3440;
  color: #e8ebf4;
}

.btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.status-message {
  margin: 1rem 0 0;
  color: #b7e7bc;
}

.error-message {
  margin: 1rem 0 0;
  color: #ff8a8a;
}

.geo-note {
  margin: 0.5rem 0 0;
  color: #b8bfd0;
  font-size: 0.85rem;
}

.restore-result {
  margin-top: 1rem;
  display: grid;
  gap: 0.35rem;
}

.restore-result p {
  margin: 0;
}

.restart-note {
  margin: 0;
  padding: 0.6rem 0.8rem;
  border-radius: 10px;
  background: rgba(90, 78, 45, 0.55);
  color: #f4de9a;
}

input[type="file"] {
  padding: 0.6rem;
}

@media (max-width: 768px) {
  .grid-2 {
    grid-template-columns: 1fr;
  }
}
</style>