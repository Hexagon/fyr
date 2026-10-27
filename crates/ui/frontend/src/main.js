import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'
import App from './App.vue'
import { i18n, initLocale } from './i18n/index.js'

import Home from './pages/Home.vue'

// Every page except the landing page is loaded on demand, so phones on a
// weak hotspot link only download MapLibre, epub.js, the crypto libraries,
// etc. when the page that needs them is opened.
const ContentManager = () => import('./pages/ContentManager.vue')
const Login = () => import('./pages/Login.vue')
const Maps = () => import('./pages/Maps.vue')
const Books = () => import('./pages/Books.vue')
const Assistant = () => import('./pages/Assistant.vue')
const Settings = () => import('./pages/Settings.vue')
const Tools = () => import('./pages/Tools.vue')
const Search = () => import('./pages/Search.vue')
const Messages = () => import('./pages/Messages.vue')
import { loadAuthStatus, useAuthState } from './services/auth'

const routes = [
  {
    path: '/',
    name: 'Home',
    component: Home,
    meta: {
      titleKey: 'routes.home.title',
      subtitleKey: 'routes.home.subtitle',
      headerLabelKey: 'routes.home.header'
    }
  },
  {
    path: '/maps',
    name: 'Maps',
    component: Maps,
    meta: {
      titleKey: 'routes.maps.title',
      subtitleKey: 'routes.maps.subtitle',
      headerLabelKey: 'routes.maps.header'
    }
  },
  {
    path: '/books',
    name: 'Books',
    component: Books,
    meta: {
      titleKey: 'routes.books.title',
      subtitleKey: 'routes.books.subtitle',
      headerLabelKey: 'routes.books.header'
    }
  },
  {
    path: '/content',
    name: 'ContentManager',
    component: ContentManager,
    meta: {
      titleKey: 'routes.content.title',
      subtitleKey: 'routes.content.subtitle',
      headerLabelKey: 'routes.content.header',
      requiresAdmin: true
    }
  },
  {
    path: '/assistant',
    name: 'Assistant',
    component: Assistant,
    meta: {
      titleKey: 'routes.assistant.title',
      subtitleKey: 'routes.assistant.subtitle',
      headerLabelKey: 'routes.assistant.header'
    }
  },
  {
    path: '/messages',
    name: 'Messages',
    component: Messages,
    meta: {
      titleKey: 'routes.messages.title',
      subtitleKey: 'routes.messages.subtitle',
      headerLabelKey: 'routes.messages.header'
    }
  },
  {
    path: '/tools',
    name: 'Tools',
    component: Tools,
    meta: {
      titleKey: 'routes.tools.title',
      subtitleKey: 'routes.tools.subtitle',
      headerLabelKey: 'routes.tools.header'
    }
  },
  {
    path: '/search',
    name: 'Search',
    component: Search,
    meta: {
      titleKey: 'routes.search.title',
      subtitleKey: 'routes.search.subtitle',
      headerLabelKey: 'routes.search.header'
    }
  },
  {
    path: '/settings',
    name: 'Settings',
    component: Settings,
    meta: {
      titleKey: 'routes.settings.title',
      subtitleKey: 'routes.settings.subtitle',
      headerLabelKey: 'routes.settings.header',
      requiresAdmin: true
    }
  },
  {
    path: '/login',
    name: 'Login',
    component: Login,
    meta: {
      titleKey: 'routes.login.title',
      subtitleKey: 'routes.login.subtitle',
      headerLabelKey: 'routes.login.header'
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
app.use(i18n)
app.use(router)
// Load the stored/detected language before the first render so a Swedish
// browser never flashes English; a failed chunk load falls back to English.
initLocale().finally(() => app.mount('#app'))

