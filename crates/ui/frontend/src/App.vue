<template>
  <div class="app-container">
    <nav class="navbar">
      <div class="navbar-brand">
        <img src="/assets/fyr-icon.svg" alt="Fyr" class="brand-icon" />
        <h1>Fyr</h1>
      </div>
      <div class="navbar-right">
        <ul class="navbar-menu">
          <li><router-link to="/" :class="{ active: $route.name === 'Home' }">Overview</router-link></li>
          <li><router-link to="/maps" :class="{ active: $route.name === 'Maps' }">Maps</router-link></li>
          <li><router-link to="/books" :class="{ active: $route.name === 'Books' }">Library</router-link></li>
          <li><router-link to="/content" :class="{ active: $route.name === 'ContentManager' }">Content Manager</router-link></li>
          <li><router-link to="/assistant" :class="{ active: $route.name === 'Assistant' }">Assistant</router-link></li>
          <li><router-link to="/settings" :class="{ active: $route.name === 'Settings' }">Settings</router-link></li>
        </ul>
        <span class="offline-pill">Offline Mode: Active</span>
        <div class="clock-panel">
          <span class="clock-time">{{ currentTime }}</span>
          <span class="clock-date">{{ currentDate }}</span>
          <span class="clock-location">{{ locationSummary }}</span>
          <span v-if="clock.sunriseText || clock.sunsetText" class="clock-sun">
            <template v-if="clock.sunriseText">↑ {{ clock.sunriseText }}</template>
            <template v-if="clock.sunriseText && clock.sunsetText"> · </template>
            <template v-if="clock.sunsetText">↓ {{ clock.sunsetText }}</template>
          </span>
        </div>
      </div>
    </nav>

    <main class="page-content">
      <section class="page-header">
        <h2>{{ pageTitle }}</h2>
        <p>{{ pageSubtitle }}</p>
      </section>
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
const clock = computed(() => getLocationClock(now.value, locationState.location))
const currentTime = computed(() => clock.value.timeText)
const currentDate = computed(() => clock.value.dateText)
const locationSummary = computed(() => {
  const location = locationState.location
  if (!location) return 'System time'

  const parts = [
    location.label?.trim(),
    `${location.latitude.toFixed(4)}, ${location.longitude.toFixed(4)}`
  ].filter(Boolean)

  return parts.join(' · ')
})

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
  background: linear-gradient(135deg, var(--navbar-gradient-1) 0%, var(--navbar-gradient-2) 100%);
  color: white;
  padding: 1rem 2rem;
  box-shadow: 0 2px 8px rgba(0,0,0,0.3);
}

.navbar-right {
  display: flex;
  align-items: center;
  gap: 1rem;
  flex-wrap: wrap;
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

.brand-icon {
  width: 1.5rem;
  height: 1.5rem;
}

.navbar-menu {
  display: flex;
  list-style: none;
  gap: 2rem;
  margin: 0;
}

.offline-pill {
  border: 1px solid #5da86c;
  color: #bdf9c6;
  background: rgba(33, 80, 45, 0.45);
  border-radius: 999px;
  padding: 0.35rem 0.7rem;
  font-size: 0.8rem;
  font-weight: 700;
  white-space: nowrap;
}

.clock-panel {
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
  min-width: 9rem;
  padding: 0.4rem 0.7rem;
  border-radius: 0.6rem;
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.08);
}

.clock-time {
  font-size: 0.95rem;
  font-weight: 700;
}

.clock-date,
.clock-location {
  font-size: 0.75rem;
  color: rgba(255, 255, 255, 0.78);
}

.clock-sun {
  font-size: 0.72rem;
  color: rgba(255, 231, 160, 0.95);
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

.page-header {
  margin-bottom: 1.25rem;
}

.page-header h2 {
  margin: 0;
  font-size: 1.75rem;
  color: #f0f0f0;
}

.page-header p {
  margin: 0.35rem 0 0;
  color: #b8b8b8;
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
  }

  .navbar-right {
    flex-direction: column;
  }

  .navbar-menu {
    flex-wrap: wrap;
    gap: 1rem;
    justify-content: center;
  }

  .page-content {
    padding: 1rem 0.85rem 1.25rem;
  }
}
</style>
