<template>
  <div class="app-container">
    <nav class="navbar">
      <div class="navbar-left">
        <div class="navbar-brand">
          <img src="/assets/fyr-icon.svg" alt="Fyr" class="brand-icon" />
          <h1>Fyr</h1>
        </div>
        <div class="navbar-context">
          <p class="context-kicker">{{ pageTitle }}</p>
          <h2>{{ headerSummary }}</h2>
        </div>
      </div>
      <div class="navbar-center">
        <ul class="navbar-menu">
          <li><router-link to="/" :class="{ active: $route.name === 'Home' }">Overview</router-link></li>
          <li><router-link to="/maps" :class="{ active: $route.name === 'Maps' }">Maps</router-link></li>
          <li><router-link to="/books" :class="{ active: $route.name === 'Books' }">Library</router-link></li>
          <li><router-link to="/content" :class="{ active: $route.name === 'ContentManager' }">Content Manager</router-link></li>
          <li><router-link to="/assistant" :class="{ active: $route.name === 'Assistant' }">Assistant</router-link></li>
          <li><router-link to="/settings" :class="{ active: $route.name === 'Settings' }">Settings</router-link></li>
        </ul>
      </div>
      <div class="navbar-right">
        <div class="clock-panel">
          <span class="clock-time">{{ currentTime }}</span>
          <span class="clock-day">{{ currentWeekday }}</span>
          <span class="clock-date">{{ currentDate }}</span>
        </div>
      </div>
    </nav>

    <main class="page-content">
      <router-view />
    </main>

    <footer class="app-footer">
      <p>Fyr v0.3.0</p>
    </footer>
  </div>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { loadAppSettings } from './services/settings'
import { useLocationState } from './services/location'
import { getLocationClock } from './services/locationClock'

const route = useRoute()
const locationState = useLocationState()
const now = ref(new Date())
let timer = null

const pageTitle = computed(() => route.meta?.title || 'Fyr')
const pageSubtitle = computed(() => route.meta?.subtitle || 'Offline-first content platform')
const pageHeaderLabel = computed(() => route.meta?.headerLabel || pageSubtitle.value)
const clock = computed(() => getLocationClock(now.value, locationState.location))
const currentTime = computed(() => clock.value.timeText)
const currentWeekday = computed(() => formatClockDatePart('weekday', clock.value.dateText))
const currentDate = computed(() => formatClockDatePart('date', clock.value.dateText))
const headerSummary = computed(() => `${pageTitle.value} - ${pageHeaderLabel.value}`)

const formatClockDatePart = (kind, fullText) => {
  const parts = String(fullText || '').split(',').map(part => part.trim()).filter(Boolean)

  if (!parts.length) {
    return ''
  }

  if (parts.length === 1) {
    return kind === 'weekday' ? parts[0] : ''
  }

  return kind === 'weekday' ? parts[0] : parts.slice(1).join(', ')
}

onMounted(async () => {
  await loadAppSettings().catch(() => {})
  timer = window.setInterval(() => {
    now.value = new Date()
  }, 1000)
})

onBeforeUnmount(() => {
  if (timer) {
    window.clearInterval(timer)
  }
})
</script>

<style scoped>
:root {
  --bg-primary: #1a1a1a;
  --bg-secondary: #2a2a2a;
  --text-primary: #e0e0e0;
  --text-secondary: #b0b0b0;
  --navbar-gradient-1: #2a2a3e;
  --navbar-gradient-2: #1a1a2e;
  --card-bg: #262626;
  --border-color: #3a3a3a;
}

.app-container {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
  background: var(--bg-primary);
  color: var(--text-primary);
}

.navbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1.25rem;
  background: linear-gradient(135deg, var(--navbar-gradient-1) 0%, var(--navbar-gradient-2) 100%);
  color: white;
  padding: 1rem 2rem;
  box-shadow: 0 2px 8px rgba(0,0,0,0.3);
}

.navbar-left {
  display: flex;
  align-items: center;
  gap: 1.25rem;
  flex-wrap: wrap;
}

.navbar-center {
  flex: 1;
  display: flex;
  justify-content: center;
}

.navbar-right {
  display: flex;
  align-items: center;
  justify-content: flex-end;
}

.navbar-brand h1 {
  font-size: 1.5rem;
  margin: 0;
}

.navbar-brand {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.navbar-context {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
}

.context-kicker {
  margin: 0;
  font-size: 0.72rem;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: rgba(255, 255, 255, 0.64);
}

.navbar-context h2 {
  margin: 0;
  font-size: 1.05rem;
  font-weight: 600;
  line-height: 1.2;
}

.brand-icon {
  width: 1.5rem;
  height: 1.5rem;
}

.navbar-menu {
  display: flex;
  list-style: none;
  gap: 2rem;
  margin: 0;
  padding: 0;
}

.clock-panel {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 0.15rem;
  min-width: 8rem;
  padding: 0.4rem 0.7rem;
  border-radius: 0.6rem;
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.08);
}

.clock-time {
  font-size: 1rem;
  font-weight: 700;
}

.clock-day,
.clock-date {
  font-size: 0.75rem;
  color: rgba(255, 255, 255, 0.78);
}

.navbar-menu a {
  color: white;
  text-decoration: none;
  font-weight: 500;
  padding: 0.5rem 1rem;
  border-radius: 4px;
  transition: background 0.3s;
}

.navbar-menu a:hover {
  background: rgba(255,255,255,0.15);
}

.navbar-menu a.active {
  background: rgba(255,255,255,0.25);
  border-bottom: 2px solid white;
}

.page-content {
  flex: 1;
  padding: 1.25rem 1.5rem 2rem;
  max-width: 100%;
  width: 100%;
  background: var(--bg-primary);
}

.app-footer {
  background: var(--bg-secondary);
  border-top: 1px solid var(--border-color);
  padding: 1rem;
  text-align: center;
  font-size: 0.9rem;
  color: var(--text-secondary);
}

.app-footer a {
  color: #667eea;
  text-decoration: none;
}

.app-footer a:hover {
  text-decoration: underline;
}

@media (max-width: 768px) {
  .navbar {
    flex-direction: column;
    gap: 1rem;
    align-items: stretch;
  }

  .navbar-left,
  .navbar-center,
  .navbar-right {
    flex-direction: column;
    align-items: stretch;
  }

  .navbar-center {
    justify-content: flex-start;
  }

  .navbar-menu {
    flex-wrap: wrap;
    gap: 1rem;
    justify-content: center;
  }

  .clock-panel {
    align-items: flex-start;
  }

  .page-content {
    padding: 1rem 0.85rem 1.25rem;
  }
}
</style>
