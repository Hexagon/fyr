<template>
  <form v-if="!onSearchPage" class="header-search" role="search" @submit.prevent="go">
    <label for="header-search-input" class="visually-hidden">Search everything offline</label>
    <input
      id="header-search-input"
      ref="inputEl"
      v-model="text"
      type="search"
      class="header-search-input"
      placeholder="Search…"
      autocomplete="off"
      spellcheck="false"
      :maxlength="MAX_QUERY_LENGTH"
      aria-keyshortcuts="/"
      title="Search books, articles, maps and places (press /)"
      @keydown.esc="onEscape"
    />
    <kbd class="header-search-key" aria-hidden="true">/</kbd>
  </form>
</template>

<script setup>
// Compact search box for the app header. Submitting navigates to
// /search?q=…, where the full results page runs the query. Pressing "/"
// anywhere (outside text fields) focuses it, or the page's own box when
// already on /search.
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { MAX_QUERY_LENGTH, normalizeQuery } from '../modules/search/searchResults.js'

const route = useRoute()
const router = useRouter()
const inputEl = ref(null)
const text = ref('')

const onSearchPage = computed(() => route.name === 'Search')

const go = () => {
  const q = normalizeQuery(text.value)
  router.push({ name: 'Search', query: q ? { q } : {} }).catch(() => {})
  text.value = ''
  inputEl.value?.blur()
}

const onEscape = () => {
  text.value = ''
  inputEl.value?.blur()
}

const isEditableTarget = (target) => {
  if (!target || !(target instanceof Element)) return false
  if (target.isContentEditable) return true
  return Boolean(target.closest('input, textarea, select, [contenteditable=""], [contenteditable="true"]'))
}

const onGlobalKeydown = (event) => {
  if (event.key !== '/' || event.ctrlKey || event.metaKey || event.altKey) return
  if (event.defaultPrevented || isEditableTarget(event.target)) return
  const target = onSearchPage.value
    ? document.getElementById('global-search-input')
    : inputEl.value
  if (!target) return
  event.preventDefault()
  target.focus()
  if (typeof target.select === 'function') target.select()
}

onMounted(() => window.addEventListener('keydown', onGlobalKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onGlobalKeydown))
</script>

<style scoped>
.header-search {
  position: relative;
  display: flex;
  align-items: center;
  min-width: 0;
}

.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
  white-space: nowrap;
}

.header-search-input {
  width: 12rem;
  max-width: 100%;
  padding: 0.45rem 2rem 0.45rem 0.75rem;
  border-radius: 0.5rem;
  border: 1px solid rgba(255, 255, 255, 0.15);
  background: rgba(255, 255, 255, 0.07);
  color: #fff;
  font-size: 0.9rem;
  transition: width 0.2s, background 0.2s;
}

.header-search-input::placeholder {
  color: rgba(255, 255, 255, 0.55);
}

.header-search-input:focus {
  width: 16rem;
  outline: 2px solid rgba(143, 168, 255, 0.8);
  outline-offset: 1px;
  background: rgba(0, 0, 0, 0.25);
}

.header-search-key {
  position: absolute;
  right: 0.55rem;
  font-size: 0.7rem;
  color: rgba(255, 255, 255, 0.55);
  border: 1px solid rgba(255, 255, 255, 0.25);
  border-radius: 3px;
  padding: 0 0.3rem;
  pointer-events: none;
}

.header-search-input:focus + .header-search-key {
  display: none;
}

@media (max-width: 768px) {
  .header-search {
    width: 100%;
  }

  .header-search-input,
  .header-search-input:focus {
    width: 100%;
  }
}
</style>
