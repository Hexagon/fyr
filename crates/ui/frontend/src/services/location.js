import { reactive, readonly } from 'vue'
import { emitAppEvent, onAppEvent } from './eventBus'

const state = reactive({
  location: null,
  source: 'system',
  updatedAt: null
})

const applyLocation = (location, source = 'manual', shouldEmit = true) => {
  state.location = location
  state.source = source
  state.updatedAt = new Date().toISOString()

  if (shouldEmit) {
    emitAppEvent('location:changed', { ...state })
  }
}

export const useLocationState = () => readonly(state)

export const setLocation = (location, source = 'manual') => {
  applyLocation(location, source, true)
}

export const clearLocation = () => {
  setLocation(null, 'system')
}

export const syncLocationFromSettings = (settings, source = 'persisted') => {
  if (settings?.location) {
    setLocation(settings.location, source)
    return
  }

  clearLocation()
}

export const publishLocationUpdate = (location, source = 'module') => {
  emitAppEvent('location:set', { location, source })
}

export const publishLocationClear = (source = 'module') => {
  emitAppEvent('location:clear', { source })
}

onAppEvent('location:set', ({ location, source } = {}) => {
  applyLocation(location ?? null, source || 'module', true)
})

onAppEvent('location:clear', ({ source } = {}) => {
  applyLocation(null, source || 'module', true)
})