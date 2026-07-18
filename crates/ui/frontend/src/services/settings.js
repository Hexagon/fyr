import { reactive, readonly } from 'vue'
import { apiService } from './api'
import { emitAppEvent } from './eventBus'
import { clearLocation, syncLocationFromSettings } from './location'

const state = reactive({
  loaded: false,
  loading: false,
  error: null,
  settings: {
    location: null,
    modules: {}
  }
})

const normalizeSettings = (payload) => ({
  location: payload?.location ?? null,
  modules: payload?.modules ?? {}
})

export const useSettingsState = () => readonly(state)

export const loadAppSettings = async () => {
  state.loading = true
  state.error = null

  try {
    const response = await apiService.getSettings()
    state.settings = normalizeSettings(response.data)
    state.loaded = true
    emitAppEvent('settings:loaded', state.settings)
    syncLocationFromSettings(state.settings, 'persisted')
    return state.settings
  } catch (error) {
    state.error = apiService.handleError(error)
    throw error
  } finally {
    state.loading = false
  }
}

export const saveSettings = async (nextSettings) => {
  state.loading = true
  state.error = null

  try {
    const response = await apiService.updateSettings(nextSettings)
    state.settings = normalizeSettings(response.data)
    state.loaded = true
    emitAppEvent('settings:updated', state.settings)
    syncLocationFromSettings(state.settings, 'manual')
    return state.settings
  } catch (error) {
    state.error = apiService.handleError(error)
    throw error
  } finally {
    state.loading = false
  }
}

export const saveLocation = async (location) => {
  return saveSettings({
    ...state.settings,
    location
  })
}

export const clearSavedLocation = async () => {
  clearLocation()
  return saveLocation(null)
}

export const updateModuleState = async (moduleName, moduleState) => {
  const nextModules = {
    ...state.settings.modules,
    [moduleName]: moduleState
  }

  return saveSettings({
    ...state.settings,
    modules: nextModules
  })
}

export const clearModuleState = async (moduleName) => {
  const nextModules = { ...state.settings.modules }
  delete nextModules[moduleName]

  return saveSettings({
    ...state.settings,
    modules: nextModules
  })
}