<template>
  <div class="settings-page">
    <section class="panel">
      <div class="panel-header">
        <h3>Location</h3>
        <span class="badge" :class="settingsState.settings.location ? 'active' : 'inactive'">
          {{ settingsState.settings.location ? 'Saved' : 'System time' }}
        </span>
      </div>

      <div class="location-summary" v-if="settingsState.settings.location">
        <p><strong>Latitude:</strong> {{ settingsState.settings.location.latitude.toFixed(6) }}</p>
        <p><strong>Longitude:</strong> {{ settingsState.settings.location.longitude.toFixed(6) }}</p>
        <p v-if="settingsState.settings.location.label"><strong>Label:</strong> {{ settingsState.settings.location.label }}</p>
      </div>

      <form class="settings-form" @submit.prevent="handleSave">
        <label>
          Label
          <input v-model="form.label" type="text" placeholder="Home, cabin, camp, ..." />
        </label>

        <div class="grid-2">
          <label>
            Latitude
            <input v-model="form.latitude" type="number" step="0.000001" placeholder="59.3293" required />
          </label>

          <label>
            Longitude
            <input v-model="form.longitude" type="number" step="0.000001" placeholder="18.0686" required />
          </label>
        </div>

        <div class="actions">
          <button type="submit" class="btn btn-primary" :disabled="saving">
            {{ saving ? 'Saving...' : 'Save location' }}
          </button>
          <button type="button" class="btn btn-secondary" @click="handleBrowserLocation" :disabled="saving || !browserLocationAvailable">
            Use browser geolocation
          </button>
          <button type="button" class="btn btn-secondary" @click="handleClear" :disabled="saving || !settingsState.settings.location">
            Clear location
          </button>
        </div>
      </form>

      <p v-if="message" class="status-message">{{ message }}</p>
      <p v-if="error" class="error-message">{{ error }}</p>
    </section>

    <section class="panel">
      <h3>Shared module bus</h3>
      <p class="body-copy">
        The settings page writes to the shared location module and backend settings store. Other modules can later emit location changes through the same bus.
      </p>
    </section>

    <section class="panel">
      <div class="panel-header">
        <h3>Module state</h3>
        <span class="badge inactive">{{ moduleCount }} modules</span>
      </div>

      <div v-if="moduleEntries.length" class="module-list">
        <article v-for="entry in moduleEntries" :key="entry.name" class="module-item">
          <div class="module-item-header">
            <strong>{{ entry.name }}</strong>
            <button type="button" class="link-button" @click="removeModule(entry.name)">Remove</button>
          </div>
          <pre>{{ entry.value }}</pre>
        </article>
      </div>
      <p v-else class="body-copy">No module state is stored yet. Future modules can write their own state here.</p>

      <div class="module-editor">
        <h4>Add or update module state</h4>
        <div class="grid-2">
          <label>
            Module name
            <input v-model="moduleForm.name" type="text" placeholder="location" />
          </label>

          <label>
            Module payload
            <input v-model="moduleForm.payload" type="text" placeholder='{"enabled": true}' />
          </label>
        </div>
        <div class="actions">
          <button type="button" class="btn btn-primary" @click="handleSaveModuleState" :disabled="saving || !moduleForm.name">
            Save module state
          </button>
        </div>
      </div>
    </section>
  </div>
</template>

<script setup>
import { computed, onMounted, reactive, ref } from 'vue'
import { apiService } from '../services/api'
import { clearModuleState, clearSavedLocation, loadAppSettings, saveLocation, updateModuleState, useSettingsState } from '../services/settings'

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

const browserLocationAvailable = computed(() => typeof navigator !== 'undefined' && 'geolocation' in navigator)
const moduleEntries = computed(() => Object.entries(settingsState.settings.modules || {}).map(([name, value]) => ({ name, value: JSON.stringify(value, null, 2) })))
const moduleCount = computed(() => moduleEntries.value.length)

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
})

const handleSave = async () => {
  message.value = ''
  error.value = ''
  saving.value = true

  const latitude = Number.parseFloat(form.latitude)
  const longitude = Number.parseFloat(form.longitude)

  if (Number.isNaN(latitude) || Number.isNaN(longitude)) {
    error.value = 'Latitude and longitude must be valid numbers.'
    saving.value = false
    return
  }

  try {
    await saveLocation({
      latitude,
      longitude,
      label: form.label.trim() || null
    })
    message.value = 'Location saved.'
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
    message.value = 'Location cleared.'
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
    error.value = 'Browser geolocation is not available in this environment.'
    saving.value = false
    return
  }

  navigator.geolocation.getCurrentPosition(
    async (position) => {
      try {
        await saveLocation({
          latitude: position.coords.latitude,
          longitude: position.coords.longitude,
          label: form.label.trim() || 'Browser location'
        })
        hydrateForm()
        message.value = 'Location captured from browser geolocation.'
      } catch (saveError) {
        error.value = apiService.handleError(saveError)
      } finally {
        saving.value = false
      }
    },
    (geoError) => {
      error.value = geoError.message || 'Unable to read browser location.'
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
    message.value = `Removed module state for ${moduleName}.`
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
    message.value = `Updated module state for ${moduleForm.name}.`
  } catch (moduleError) {
    error.value = moduleError instanceof SyntaxError
      ? 'Module payload must be valid JSON.'
      : apiService.handleError(moduleError)
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

@media (max-width: 768px) {
  .grid-2 {
    grid-template-columns: 1fr;
  }
}
</style>