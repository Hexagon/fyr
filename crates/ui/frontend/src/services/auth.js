import { reactive, readonly } from 'vue'
import axios from 'axios'

const api = axios.create({ baseURL: '/api' })

const state = reactive({
  loaded: false,
  readonly: false,
  requiresAuth: false,
  authenticated: false
})

/**
 * Fetch authentication / access-mode status from the server.
 * Called once on app startup so every component can react to the mode.
 */
export const loadAuthStatus = async () => {
  try {
    const response = await api.get('/auth/status')
    state.readonly = response.data.readonly ?? false
    state.requiresAuth = response.data.requires_auth ?? false
    state.authenticated = response.data.authenticated ?? false
    state.loaded = true
  } catch {
    // If the endpoint is missing (old server) treat as fully open.
    state.readonly = false
    state.requiresAuth = false
    state.authenticated = false
    state.loaded = true
  }
}

/**
 * Attempt to log in with the supplied password.
 * Returns `true` on success, throws with an error message on failure.
 */
export const login = async (password) => {
  const response = await api.post('/auth/login', { password })
  if (response.status === 200) {
    state.authenticated = true
    return true
  }
  throw new Error('Login failed')
}

/**
 * Log out the current admin session.
 */
export const logout = async () => {
  try {
    await api.post('/auth/logout')
  } finally {
    state.authenticated = false
  }
}

/** Reactive, read-only view of the auth state. */
export const useAuthState = () => readonly(state)

/** Returns true when admin features should be hidden / blocked for this user. */
export const isAdminLocked = () => {
  if (!state.loaded) return false
  if (state.readonly) return true
  if (state.requiresAuth && !state.authenticated) return true
  return false
}
