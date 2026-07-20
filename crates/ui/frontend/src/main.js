import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'
import App from './App.vue'

import Home from './pages/Home.vue'
import ContentManager from './pages/ContentManager.vue'
import Login from './pages/Login.vue'
import Maps from './pages/Maps.vue'
import Books from './pages/Books.vue'
import Assistant from './pages/Assistant.vue'
import Settings from './pages/Settings.vue'
import { loadAuthStatus, useAuthState } from './services/auth'

const routes = [
  {
    path: '/',
    name: 'Home',
    component: Home,
    meta: {
      title: 'Overview',
      subtitle: 'System status and offline storage overview',
      headerLabel: 'System status'
    }
  },
  {
    path: '/maps',
    name: 'Maps',
    component: Maps,
    meta: {
      title: 'Maps',
      subtitle: 'Browse offline maps and interactive data layers',
      headerLabel: 'Interactive layers'
    }
  },
  {
    path: '/books',
    name: 'Books',
    component: Books,
    meta: {
      title: 'Library',
      subtitle: 'Browse and read your offline books collection',
      headerLabel: 'Offline reading'
    }
  },
  {
    path: '/content',
    name: 'ContentManager',
    component: ContentManager,
    meta: {
      title: 'Content Manager',
      subtitle: 'Manage local files, imports, and downloads',
      headerLabel: 'Files and imports',
      requiresAdmin: true
    }
  },
  {
    path: '/assistant',
    name: 'Assistant',
    component: Assistant,
    meta: {
      title: 'Assistant',
      subtitle: 'Offline AI assistant powered by local GGUF models',
      headerLabel: 'Local GGUF chat'
    }
  },
  {
    path: '/settings',
    name: 'Settings',
    component: Settings,
    meta: {
      title: 'Settings',
      subtitle: 'Configure location and other application-wide preferences',
      headerLabel: 'Location and preferences'
    }
  },
  {
    path: '/login',
    name: 'Login',
    component: Login,
    meta: {
      title: 'Admin Login',
      subtitle: 'Authenticate to access admin features',
      headerLabel: 'Admin access'
    }
  }
]

const router = createRouter({
  history: createWebHistory(),
  routes
})

// Navigation guard: redirect to /login when a route requires admin and the
// user is not authenticated.
router.beforeEach(async (to) => {
  if (!to.meta.requiresAdmin) return true

  const auth = useAuthState()
  // Ensure auth status has been loaded at least once before guarding.
  if (!auth.loaded) {
    await loadAuthStatus()
  }

  if (auth.readonly) {
    // Strict read-only — redirect to home rather than login.
    return { name: 'Home' }
  }

  if (auth.requiresAuth && !auth.authenticated) {
    return { name: 'Login', query: { redirect: to.fullPath } }
  }

  return true
})

const app = createApp(App)
app.use(router)
app.mount('#app')

