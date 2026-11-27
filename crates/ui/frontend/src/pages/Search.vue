<template>
  <div class="search-page">
    <form class="search-bar" role="search" @submit.prevent="submitNow">
      <label for="global-search-input" class="visually-hidden">{{ t('headerSearch.label') }}</label>
      <input
        id="global-search-input"
        ref="inputEl"
        v-model="query"
        type="search"
        class="search-input"
        :placeholder="t('search.placeholder')"
        autocomplete="off"
        spellcheck="false"
        role="combobox"
        aria-autocomplete="list"
        aria-controls="search-results"
        :aria-expanded="String(groups.length > 0)"
        :aria-activedescendant="selected >= 0 ? `search-result-${selected}` : undefined"
        :maxlength="MAX_QUERY_LENGTH"
        @input="onInput"
        @keydown="onKeydown"
      />
      <button type="submit" class="search-submit" :disabled="!normalizedQuery">{{ t('common.search') }}</button>
    </form>

    <div class="search-filters" role="group" :aria-label="t('search.filterLabel')">
      <button
        v-for="filter in KIND_FILTERS"
        :key="filter.id"
        type="button"
        class="filter-chip"
        :class="{ active: activeFilter === filter.id }"
        :aria-pressed="String(activeFilter === filter.id)"
        @click="setFilter(filter.id)"
      >
        {{ t(filter.labelKey) }}
      </button>
    </div>

    <p class="search-status" aria-live="polite">
      <template v-if="loading">{{ t('common.searching') }}</template>
      <template v-else-if="response && groups.length">
        {{ statusText }}
      </template>
    </p>

    <div v-if="error" class="search-message search-error" role="alert">
      <p>{{ error }}</p>
      <button type="button" class="retry-btn" @click="retry">{{ t('common.tryAgain') }}</button>
    </div>

    <div v-else-if="!activeQuery" class="search-message search-idle">
      <h3>{{ t('search.idle.title') }}</h3>
      <p>{{ t('search.idle.body') }}</p>
      <i18n-t keypath="search.idle.tip" tag="p" class="hint" scope="global">
        <template #slash><kbd>/</kbd></template>
        <template #arrows><kbd>↑</kbd> <kbd>↓</kbd></template>
        <template #enter><kbd>Enter</kbd></template>
      </i18n-t>
    </div>

    <div v-else-if="!loading && response && !groups.length" class="search-message search-empty">
      <h3>{{ t('search.empty.title', { query: activeQuery }) }}</h3>
      <i18n-t keypath="search.empty.body" tag="p" scope="global">
        <template #all><em>{{ t('search.filter.all') }}</em></template>
      </i18n-t>
      <p v-if="response.partial" class="hint">{{ t('search.empty.partial') }}</p>
    </div>

    <div
      v-if="groups.length"
      id="search-results"
      class="search-results"
      :class="{ stale: loading }"
      role="listbox"
      :aria-label="t('search.resultsLabel')"
    >
      <section
        v-for="group in groups"
        :key="group.kind"
        class="result-group"
        role="group"
        :aria-labelledby="`search-group-${group.kind}`"
      >
        <h3 :id="`search-group-${group.kind}`" class="group-title">
          <span aria-hidden="true">{{ group.icon }}</span>
          {{ kindLabel(group.kind) }}
          <span class="group-count">{{ group.items.length }}</span>
        </h3>
        <ul class="result-list" role="presentation">
          <li v-for="item in group.items" :key="item.link" role="presentation">
            <a
              :id="`search-result-${item.index}`"
              :ref="(el) => setItemRef(item.index, el)"
              :href="item.link"
              class="result-item"
              :class="{ selected: item.index === selected }"
              role="option"
              :aria-selected="String(item.index === selected)"
              tabindex="-1"
              @click.prevent="openResult(item)"
              @mousemove="selected = item.index"
            >
              <span class="result-title">
                <template v-for="(segment, i) in highlightSegments(item.title, activeQuery)" :key="i">
                  <mark v-if="segment.match">{{ segment.text }}</mark>
                  <template v-else>{{ segment.text }}</template>
                </template>
              </span>
              <span v-if="item.subtitle" class="result-subtitle">
                <template v-for="(segment, i) in highlightSegments(item.subtitle, activeQuery)" :key="i">
                  <mark v-if="segment.match">{{ segment.text }}</mark>
                  <template v-else>{{ segment.text }}</template>
                </template>
              </span>
              <span v-if="item.snippet" class="result-snippet">
                <template v-for="(segment, i) in item.snippet" :key="i">
                  <mark v-if="segment.match">{{ segment.text }}</mark>
                  <template v-else>{{ segment.text }}</template>
                </template>
              </span>
              <span class="result-source">{{ item.source }}</span>
            </a>
          </li>
        </ul>
      </section>
    </div>

    <p v-if="response?.partial && groups.length" class="search-partial">
      {{ t('search.partial') }}
    </p>
  </div>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { apiService } from '../services/api'
import {
  KIND_FILTERS,
  MAX_QUERY_LENGTH,
  SEARCH_DEBOUNCE_MS,
  countGroupedItems,
  debounce,
  describeSearchError,
  groupResults,
  highlightSegments,
  isSafeLink,
  itemAtIndex,
  kindLabel,
  moveSelection,
  normalizeQuery,
  normalizeResults
} from '../modules/search/searchResults.js'

const RESULT_LIMIT = 60

const { t, n } = useI18n()
const route = useRoute()
const router = useRouter()

const inputEl = ref(null)
const query = ref(normalizeQuery(route.query.q))
const activeQuery = ref('')
const response = ref(null)
const groups = ref([])
const loading = ref(false)
const error = ref('')
const selected = ref(-1)
const itemRefs = new Map()

let controller = null
let requestSeq = 0

const normalizedQuery = computed(() => normalizeQuery(query.value))
const activeFilter = computed(() => {
  const kinds = String(route.query.kinds || '')
  return KIND_FILTERS.find((f) => f.kinds && f.kinds === kinds)?.id || 'all'
})
const resultCount = computed(() => countGroupedItems(groups.value))
const statusText = computed(() => {
  const data = response.value
  if (!data) return ''
  const shown = resultCount.value
  const parts = [t('search.status.results', { count: shown }, shown)]
  if (data.truncated) parts.push(t('search.status.best', { shown, total: n(Number(data.total) || 0) }))
  if (Number.isFinite(data.elapsed_ms)) parts.push(t('search.status.time', { ms: data.elapsed_ms }))
  return parts.join(' ')
})

const setItemRef = (index, el) => {
  if (el) itemRefs.set(index, el)
  else itemRefs.delete(index)
}

const routeQueryFor = (q, filterId = activeFilter.value) => {
  const next = {}
  if (q) next.q = q
  const kinds = KIND_FILTERS.find((f) => f.id === filterId)?.kinds
  if (kinds) next.kinds = kinds
  return next
}

// The route is the single source of truth: typing updates ?q= (debounced),
// and the watcher below runs the search. The header search box navigates
// here the same way.
const syncRoute = () => {
  const q = normalizedQuery.value
  if (q === normalizeQuery(route.query.q)) return
  router.replace({ name: 'Search', query: routeQueryFor(q) }).catch(() => {})
}
const debouncedSync = debounce(syncRoute, SEARCH_DEBOUNCE_MS)

const onInput = () => {
  debouncedSync()
}

// Enter / Search button: search immediately, re-running the current query
// if it is unchanged (e.g. after an error or new content was added).
const submitNow = () => {
  debouncedSync.cancel()
  const q = normalizedQuery.value
  if (q && q === normalizeQuery(route.query.q)) {
    runSearch(q, route.query.kinds)
    return
  }
  syncRoute()
}

const retry = () => {
  const q = normalizeQuery(route.query.q)
  if (q) runSearch(q, route.query.kinds)
}

const setFilter = (filterId) => {
  debouncedSync.cancel()
  router.replace({ name: 'Search', query: routeQueryFor(normalizedQuery.value, filterId) }).catch(() => {})
}

const resetResults = () => {
  response.value = null
  groups.value = []
  selected.value = -1
}

const runSearch = async (q, kinds) => {
  if (controller) controller.abort()
  error.value = ''

  if (!q) {
    controller = null
    loading.value = false
    activeQuery.value = ''
    resetResults()
    return
  }

  const seq = ++requestSeq
  controller = new AbortController()
  loading.value = true
  activeQuery.value = q

  try {
    const data = await apiService.search(q, {
      limit: RESULT_LIMIT,
      kinds: kinds || '',
      signal: controller.signal
    })
    if (seq !== requestSeq) return
    response.value = data || null
    groups.value = groupResults(normalizeResults(data))
    selected.value = -1
  } catch (err) {
    if (seq !== requestSeq || err?.name === 'CanceledError' || err?.code === 'ERR_CANCELED') return
    resetResults()
    error.value = describeSearchError(err)
  } finally {
    if (seq === requestSeq) loading.value = false
  }
}

const openResult = (item) => {
  if (!item || !isSafeLink(item.link)) return
  router.push(item.link).catch(() => {})
}

const scrollSelectedIntoView = () => {
  nextTick(() => {
    const el = itemRefs.get(selected.value)
    if (el && typeof el.scrollIntoView === 'function') {
      el.scrollIntoView({ block: 'nearest' })
    }
  })
}

const onKeydown = (event) => {
  const count = resultCount.value
  switch (event.key) {
    case 'ArrowDown':
    case 'ArrowUp':
      if (!count) return
      event.preventDefault()
      selected.value = moveSelection(selected.value, event.key === 'ArrowDown' ? 1 : -1, count)
      scrollSelectedIntoView()
      break
    case 'Enter':
      if (selected.value >= 0) {
        event.preventDefault()
        openResult(itemAtIndex(groups.value, selected.value))
      }
      // Otherwise the form submits and runs the search immediately.
      break
    case 'Escape':
      if (selected.value >= 0) {
        event.preventDefault()
        selected.value = -1
      } else if (query.value) {
        event.preventDefault()
        query.value = ''
        submitNow()
      }
      break
    default:
      break
  }
}

watch(
  () => [route.query.q, route.query.kinds],
  ([q, kinds]) => {
    if (route.name !== 'Search') return
    const next = normalizeQuery(q)
    // Keep the box in sync when navigating here from the header or history,
    // without clobbering what the user is still typing.
    if (next !== normalizedQuery.value) {
      debouncedSync.cancel()
      query.value = next
    }
    runSearch(next, kinds)
  },
  { immediate: true }
)

onMounted(() => {
  inputEl.value?.focus()
})

onBeforeUnmount(() => {
  debouncedSync.cancel()
  if (controller) controller.abort()
  requestSeq += 1
})
</script>

<style scoped>
.search-page {
  --panel: #1f2428;
  --panel-ink: #12161a;
  --line: #39434c;
  --text: #e7edf3;
  --muted: #a8b2bc;
  --accent: #8fa8ff;

  max-width: 960px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
}

.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
  white-space: nowrap;
}

.search-bar {
  display: flex;
  gap: 0.5rem;
}

.search-input {
  flex: 1;
  min-width: 0;
  font-size: 1.1rem;
  padding: 0.75rem 1rem;
  border-radius: 10px;
  border: 1px solid var(--line);
  background: var(--panel-ink);
  color: var(--text);
}

.search-input:focus {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.search-submit,
.retry-btn {
  padding: 0 1.1rem;
  border-radius: 10px;
  border: 1px solid #4f5d68;
  background: #2b3540;
  color: var(--text);
  font-weight: 600;
  cursor: pointer;
}

.retry-btn {
  padding: 0.4rem 0.9rem;
}

.search-submit:disabled {
  opacity: 0.5;
  cursor: default;
}

.search-filters {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
}

.filter-chip {
  border: 1px solid var(--line);
  background: transparent;
  color: var(--muted);
  border-radius: 999px;
  padding: 0.25rem 0.8rem;
  font-size: 0.85rem;
  cursor: pointer;
}

.filter-chip.active {
  background: #2b3540;
  border-color: var(--accent);
  color: var(--text);
}

.search-status {
  margin: 0;
  min-height: 1.2em;
  font-size: 0.85rem;
  color: var(--muted);
}

.search-message {
  background: linear-gradient(180deg, var(--panel) 0%, var(--panel-ink) 100%);
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 1.25rem 1.5rem;
  color: var(--text);
}

.search-message h3 {
  margin: 0 0 0.5rem;
  font-size: 1.05rem;
}

.search-message p {
  margin: 0.35rem 0;
  color: var(--muted);
  line-height: 1.5;
}

.search-error {
  border-color: #7f1d1d;
}

.search-error p {
  color: #fca5a5;
}

.hint kbd {
  border: 1px solid var(--line);
  border-bottom-width: 2px;
  border-radius: 4px;
  padding: 0 0.3rem;
  font-size: 0.8rem;
  color: var(--text);
}

.search-results {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  transition: opacity 0.15s;
}

.search-results.stale {
  opacity: 0.6;
}

.result-group {
  background: linear-gradient(180deg, var(--panel) 0%, var(--panel-ink) 100%);
  border: 1px solid var(--line);
  border-radius: 10px;
  overflow: hidden;
}

.group-title {
  margin: 0;
  padding: 0.6rem 1rem;
  font-size: 0.8rem;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--muted);
  border-bottom: 1px solid var(--line);
  display: flex;
  align-items: center;
  gap: 0.4rem;
}

.group-count {
  margin-left: auto;
  font-variant-numeric: tabular-nums;
}

.result-list {
  list-style: none;
  margin: 0;
  padding: 0;
}

.result-item {
  display: grid;
  grid-template-columns: 1fr auto;
  grid-template-areas:
    'title source'
    'subtitle subtitle'
    'snippet snippet';
  gap: 0.15rem 1rem;
  padding: 0.6rem 1rem;
  color: var(--text);
  text-decoration: none;
  border-left: 3px solid transparent;
}

.result-item + .result-item,
li + li .result-item {
  border-top: 1px solid rgba(255, 255, 255, 0.04);
}

.result-item.selected,
.result-item:hover {
  background: rgba(143, 168, 255, 0.1);
  border-left-color: var(--accent);
}

.result-title {
  grid-area: title;
  font-weight: 600;
  overflow-wrap: anywhere;
}

.result-subtitle {
  grid-area: subtitle;
  font-size: 0.85rem;
  color: var(--muted);
  overflow-wrap: anywhere;
}

.result-snippet {
  grid-area: snippet;
  font-size: 0.85rem;
  color: var(--text);
  opacity: 0.85;
  line-height: 1.45;
  overflow-wrap: anywhere;
}

.result-source {
  grid-area: source;
  font-size: 0.75rem;
  color: var(--muted);
  max-width: 16rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  align-self: center;
}

mark {
  background: rgba(250, 204, 21, 0.25);
  color: inherit;
  border-radius: 2px;
}

.search-partial {
  margin: 0;
  font-size: 0.85rem;
  color: #fcd34d;
}

@media (max-width: 768px) {
  .result-item {
    grid-template-columns: 1fr;
    grid-template-areas:
      'title'
      'subtitle'
      'snippet'
      'source';
  }

  .result-source {
    max-width: 100%;
  }
}
</style>
