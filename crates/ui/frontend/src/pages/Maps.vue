<template>
  <div class="maps-page">
    <div class="maps-container">
      <div id="map" class="map-canvas"></div>

      <div class="overlay overlay-selector" :class="{ collapsed: !showSelector }">
        <button
          class="overlay-toggle"
          :aria-label="showSelector ? 'Hide available maps panel' : 'Show available maps panel'"
          :aria-expanded="String(showSelector)"
          aria-controls="maps-selector-panel"
          @click="showSelector = !showSelector"
          :title="showSelector ? 'Hide maps' : 'Show maps'"
        >
          {{ showSelector ? '🗺️' : '📂' }}
        </button>
        <div id="maps-selector-panel" v-show="showSelector" class="overlay-content">
          <h3>Available Maps</h3>
          <p v-if="mapsError" class="error-state">{{ mapsError }}</p>
          <div v-else-if="mapsLoading" class="status-state">Loading maps...</div>
          <div v-else-if="maps.length" class="maps-list">
            <button
              v-for="map in maps"
              :key="map.filename"
              @click="selectMap(map)"
              class="map-item"
              :class="{ active: selectedMap?.filename === map.filename }"
            >
              <span class="map-name">{{ map.filename }}</span>
              <span class="map-size">{{ formatBytes(map.size) }}</span>
            </button>
          </div>
          <p v-else class="empty-state">
            No maps available. <router-link to="/content">Add maps</router-link>
          </p>
        </div>
      </div>

      <div v-if="selectedMap" class="overlay overlay-layers" :class="{ collapsed: !showLayers }">
        <button
          class="overlay-toggle"
          :aria-label="showLayers ? 'Hide layers panel' : 'Show layers panel'"
          :aria-expanded="String(showLayers)"
          aria-controls="maps-layers-panel"
          @click="showLayers = !showLayers"
          :title="showLayers ? 'Hide layers' : 'Show layers'"
        >
          {{ showLayers ? '🎨' : '🧩' }}
        </button>
        <div id="maps-layers-panel" v-show="showLayers" class="overlay-content">
          <h3>Map Layers</h3>
          <p class="overlay-meta" v-if="renderMode === 'raster'">
            Raster mode detected. Vector layer controls are disabled.
          </p>
          <div class="layer-toggles">
            <label
              v-for="toggle in layerToggles"
              :key="toggle.group"
              class="layer-toggle"
              :class="{ disabled: !isVectorMode }"
            >
              <input
                type="checkbox"
                :disabled="!isVectorMode"
                v-model="layerVisibility[toggle.group]"
                @change="toggleLayer(toggle.group)"
              >
              <span class="layer-name">{{ toggle.label }}</span>
            </label>
          </div>
        </div>
      </div>

      <div v-if="selectedMap" class="overlay overlay-info" :class="{ collapsed: !showInfo }">
        <button
          class="overlay-toggle"
          :aria-label="showInfo ? 'Hide map info panel' : 'Show map info panel'"
          :aria-expanded="String(showInfo)"
          aria-controls="maps-info-panel"
          @click="showInfo = !showInfo"
          :title="showInfo ? 'Hide info' : 'Show info'"
        >
          {{ showInfo ? 'ℹ️' : '📍' }}
        </button>
        <div id="maps-info-panel" v-show="showInfo" class="overlay-content">
          <h3>{{ selectedMap.filename }}</h3>
          <p v-if="selectedMap.path && selectedMap.path !== selectedMap.filename" class="overlay-meta">📍 {{ selectedMap.path }}</p>
          <p class="overlay-meta">📊 {{ formatBytes(selectedMap.size) }}</p>
          <p class="overlay-meta">⏰ {{ new Date(selectedMap.modified).toLocaleDateString() }}</p>
          <p class="overlay-meta">Mode: {{ renderModeLabel }}</p>
          <p v-if="selectedMapBounds" class="overlay-meta">
            Coverage: {{ formatBoundsText(selectedMapBounds) }}
          </p>
        </div>
      </div>

      <div v-if="!selectedMap" class="empty-view">
        <p>{{ mapsLoading ? 'Loading maps...' : '👈 Select a map to view' }}</p>
      </div>

      <div v-if="mapError" class="map-error-banner">{{ mapError }}</div>

      <div v-if="selectedMap" class="overlay overlay-tools" :class="{ collapsed: !showTools }">
        <button
          class="overlay-toggle"
          :aria-label="showTools ? 'Hide tools panel' : 'Show tools panel'"
          :aria-expanded="String(showTools)"
          aria-controls="maps-tools-panel"
          @click="showTools = !showTools"
          :title="showTools ? 'Hide tools' : 'Show tools'"
        >
          {{ showTools ? '🔧' : '⚙️' }}
        </button>
        <div id="maps-tools-panel" v-show="showTools" class="overlay-content">
          <h3>Tools</h3>

          <div class="tool-group">
            <button
              class="tool-btn"
              :class="{ loading: locating }"
              :disabled="locating"
              @click="centerOnUserLocation"
              title="Center map on your current browser location"
            >
              📍 {{ locating ? 'Locating…' : 'My Location' }}
            </button>
            <p v-if="locationError" class="tool-error">{{ locationError }}</p>
          </div>

          <div class="tool-group">
            <form class="goto-row" @submit.prevent="goToCoordinate">
              <input
                v-model="gotoInput"
                class="poi-input"
                placeholder="Go to: 59.33, 18.07 or 59°19'N 18°4'E"
                aria-label="Go to coordinate"
                autocomplete="off"
                spellcheck="false"
              />
              <button class="tool-btn-secondary" type="submit" :disabled="!gotoInput.trim()">Go</button>
            </form>
            <p v-if="gotoError" class="tool-error">{{ gotoError }}</p>
            <div v-if="deepLinkTarget" class="marker-row">
              <span class="tool-hint" :title="markerSummary">📌 {{ markerSummary }}</span>
              <button class="tool-btn-icon" @click="clearDeepLinkMarker" title="Remove marker">✕</button>
            </div>
          </div>

          <div class="tool-group">
            <button
              class="tool-btn"
              :class="{ active: activeTool === 'measure' }"
              @click="toggleTool('measure')"
              title="Measure distance by clicking points on the map"
            >
              📏 {{ activeTool === 'measure' ? 'Stop Measuring' : 'Measure Distance' }}
            </button>
            <div v-if="activeTool === 'measure'" class="tool-sub-panel">
              <p class="tool-hint">Click the map to add waypoints.</p>
              <div v-if="measurePoints.length === 0" class="tool-dim">No points yet</div>
              <div v-else class="measure-info">
                <span>{{ measurePoints.length }} point{{ measurePoints.length !== 1 ? 's' : '' }}</span>
                <span class="measure-distance">{{ formatDistance(measureTotalDistance) }}</span>
              </div>
              <button
                class="tool-btn-secondary"
                :disabled="measurePoints.length === 0"
                @click="clearMeasure"
              >Clear</button>
            </div>
          </div>

          <div class="tool-group">
            <button
              class="tool-btn"
              :class="{ active: trackState.recording }"
              @click="toggleRecording"
              :title="trackState.recording ? 'Stop recording and keep the track' : 'Record your GPS track'"
            >
              {{ trackState.recording ? '⏹ Stop Recording' : '⏺ Record Track' }}
            </button>
            <div v-if="trackState.recording || finishedTrack" class="tool-sub-panel">
              <div class="measure-info">
                <span>{{ formatDuration(trackState.durationMs) }} · {{ trackState.points }} pt{{ trackState.points !== 1 ? 's' : '' }}</span>
                <span class="measure-distance">{{ formatDistance(trackState.distanceKm) }}</span>
              </div>
              <p v-if="trackState.recording" class="tool-hint">
                Recording{{ trackState.accuracy !== null ? ` · ±${Math.round(trackState.accuracy)} m` : '' }}. Keep this tab open and the screen on.
              </p>
              <p v-if="trackState.recovered" class="tool-hint">Recovered an unsaved track from your last session.</p>
              <template v-if="finishedTrack && !trackState.recording">
                <input v-model="trackName" class="poi-input" placeholder="Track name" aria-label="Track name" />
                <template v-if="canEdit">
                  <div class="poi-create-row">
                    <input
                      v-model="trackTargetFile"
                      class="poi-input"
                      list="maps-poi-file-options"
                      placeholder="tracks.geojson"
                      aria-label="Save track to POI file"
                    />
                    <button class="tool-btn-secondary" :disabled="trackSaving" @click="saveTrack">
                      {{ trackSaving ? 'Saving…' : 'Save' }}
                    </button>
                  </div>
                  <datalist id="maps-poi-file-options">
                    <option v-for="f in poiFileOptions" :key="f.filename" :value="f.filename" />
                  </datalist>
                </template>
                <p v-else class="tool-hint">Read-only mode: download the track, or sign in as admin to save it on the server.</p>
                <div class="tool-row">
                  <button class="tool-btn-secondary" @click="downloadTrackGpx">⬇ GPX</button>
                  <button class="tool-btn-secondary" @click="discardTrack">Discard</button>
                </div>
              </template>
            </div>
            <p v-if="trackState.error" class="tool-error">{{ trackState.error }}</p>
            <p v-else-if="trackState.warning" class="tool-warning">{{ trackState.warning }}</p>
            <p v-if="trackMessage" :class="trackMessage.type === 'error' ? 'tool-error' : 'tool-success'">{{ trackMessage.text }}</p>
          </div>

          <div class="tool-group">
            <button
              class="tool-btn"
              :class="{ active: activeTool === 'poi' }"
              @click="toggleTool('poi')"
              :title="canEdit ? 'Add and edit custom Points of Interest' : 'View Points of Interest and tracks'"
            >
              🏷️ {{ activeTool === 'poi' ? 'Close POI Panel' : (canEdit ? 'Edit POIs' : 'View POIs') }}
            </button>
            <div v-if="activeTool === 'poi'" class="tool-sub-panel">
              <div class="poi-file-row">
                <select v-model="selectedPoiFile" @change="onPoiFileChange" class="poi-select" aria-label="POI file">
                  <option value="">— select file —</option>
                  <option v-for="f in poiFileOptions" :key="f.filename" :value="f.filename">
                    {{ f.filename }}{{ f.unsaved ? ' (not saved)' : '' }}
                  </option>
                </select>
                <button
                  v-if="canEdit"
                  class="tool-btn-icon"
                  :class="{ active: showNewPoiFile }"
                  @click="showNewPoiFile = !showNewPoiFile"
                  title="Create new POI file"
                >+</button>
              </div>
              <div v-if="showNewPoiFile && canEdit" class="poi-create-row">
                <input
                  v-model="newPoiFileName"
                  placeholder="name.geojson"
                  class="poi-input"
                  @keydown.enter="createPoiFile"
                  @keydown.escape="showNewPoiFile = false"
                />
                <button class="tool-btn-secondary" :disabled="!newPoiFileName.trim()" @click="createPoiFile">Create</button>
              </div>
              <div class="tool-row">
                <button class="tool-btn-secondary" @click="openGpxPicker" title="Import waypoints, tracks and routes from a .gpx file">⬆ Import GPX</button>
                <button
                  class="tool-btn-secondary"
                  :disabled="!poiData || poiData.features.length === 0"
                  @click="exportPoiGpx"
                  title="Download as GPX"
                >⬇ GPX</button>
                <button
                  class="tool-btn-secondary"
                  :disabled="!poiData || poiData.features.length === 0"
                  @click="exportPoiGeoJson"
                  title="Download as GeoJSON"
                >⬇ GeoJSON</button>
                <input
                  ref="gpxInput"
                  type="file"
                  accept=".gpx,application/gpx+xml,application/xml,text/xml"
                  class="visually-hidden"
                  @change="onGpxFileSelected"
                />
              </div>
              <p v-if="poiMessage" :class="poiMessage.type === 'error' ? 'tool-error' : 'tool-success'">{{ poiMessage.text }}</p>
              <template v-if="selectedPoiFile && poiData">
                <p class="tool-hint">
                  <template v-if="canEdit">Click the map to place a POI. </template>
                  {{ poiData.features.length }} feature{{ poiData.features.length !== 1 ? 's' : '' }} loaded.
                </p>
                <div v-if="pendingPoiCoords" class="poi-name-row">
                  <input
                    ref="poiNameInput"
                    v-model="pendingPoiName"
                    placeholder="POI name"
                    class="poi-input"
                    @keydown.enter="confirmPoi"
                    @keydown.escape="cancelPoi"
                  />
                  <button class="tool-btn-secondary" :disabled="!pendingPoiName.trim()" @click="confirmPoi">Add</button>
                  <button class="tool-btn-icon" @click="cancelPoi" title="Cancel">✕</button>
                </div>
                <div v-if="poiData.features.length > 0" class="poi-list">
                  <div v-for="(feat, idx) in poiData.features" :key="idx" class="poi-list-item">
                    <button class="poi-list-name" @click="zoomToFeature(feat)" :title="'Show ' + featureLabel(feat)">
                      {{ featureIcon(feat) }} {{ featureLabel(feat) }}
                    </button>
                    <button v-if="canEdit" class="poi-remove-btn" @click="removePoi(idx)" title="Remove">✕</button>
                  </div>
                </div>
                <div v-if="canEdit" class="poi-save-row">
                  <button
                    class="tool-btn"
                    :class="{ saved: !poiDirty }"
                    :disabled="!poiDirty || poiSaving"
                    @click="savePoiFile"
                  >
                    {{ poiSaving ? 'Saving…' : poiDirty ? 'Save Changes' : 'Saved ✓' }}
                  </button>
                </div>
                <p v-else-if="poiDirty" class="tool-hint">Read-only mode: imported data is shown but not saved.</p>
              </template>
            </div>
          </div>

        </div>
      </div>

      <div class="map-note" v-if="selectedMap">
        <button
          v-if="displayCoord"
          class="coord-readout"
          @click="copyDisplayedCoordinate"
          :title="`${displayCoord.source === 'cursor' ? 'Cursor' : 'Map center'} — click to copy`"
        >
          <span class="coord-source">{{ displayCoord.source === 'cursor' ? '🖱' : '⊕' }}</span>
          {{ displayCoordText }}
          <span v-if="coordCopied" class="coord-copied">Copied</span>
        </button>
        <button
          class="coord-mode-btn"
          @click="toggleCoordMode"
          :title="coordMode === 'dms' ? 'Show decimal degrees' : 'Show degrees, minutes, seconds'"
        >{{ coordMode === 'dms' ? 'DMS' : 'DEC' }}</button>
        <span class="map-note-mode">
          {{ renderMode === 'vector'
            ? 'Vector tile mode. Layers can be styled and toggled.'
            : renderMode === 'raster'
              ? 'Raster tile mode. Tiles are pre-rendered and styling controls are limited.'
              : 'Detecting tile mode.' }}
        </span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, watch, onBeforeUnmount, computed, nextTick } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { apiService } from '../services/api'
import { useLocationState } from '../services/location'
import { isAdminLocked } from '../services/auth'
import * as maplibregl from 'maplibre-gl'
// MapLibre 6 runs its renderer worker from a separate module file. Ship it
// as a local asset (Fyr is offline) and point MapLibre at it explicitly.
import maplibreWorkerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?url'
import { PMTiles, Protocol } from 'pmtiles'
import 'maplibre-gl/dist/maplibre-gl.css'
import {
  formatBytes,
  formatDistance,
  formatDuration,
  pathLength,
  boundsContain
} from '../modules/maps/geo'
import { buildMapDataUrl as buildMapDataUrlFor, mbtilesMetadataUrl } from '../modules/maps/mapFiles'
import { loadMapSource } from '../modules/maps/mapSources'
import {
  emptyGeoJSON,
  normalizePoiData,
  normalizePoiFilename,
  uniquePoiFilename,
  slugifyFilename,
  makePointFeature,
  buildMeasureGeoJSON
} from '../modules/maps/poi'
import {
  MEASURE_SOURCE_ID,
  POI_SOURCE_ID,
  TRACK_SOURCE_ID,
  POI_INTERACTIVE_LAYER_IDS,
  layerGroups,
  defaultLayerVisibility,
  buildBaseStyle,
  buildMeasureLayers,
  buildPoiLayers,
  poiLayerIds,
  buildTrackLayers
} from '../modules/maps/style'
import { formatCoordinate, formatDecimal, formatDms, parseCoordinate } from '../modules/maps/coords'
import { parseMapsQuery, buildMapsQuery, deepLinkKey, DEFAULT_POINT_ZOOM } from '../modules/maps/deepLink'
import { createBoundsResolver } from '../modules/maps/bounds'
import { parseGpx, toGpx } from '../modules/maps/gpx'
import { checkGeolocationSupport, geolocationErrorMessage, defaultTrackName, trackToFeature, trackToGeoJSON } from '../modules/maps/tracking'
import { getTrackRecorder } from '../modules/maps/useTrackRecorder'
import { copyText, downloadText, readFileText } from '../modules/maps/browser'

maplibregl.setWorkerUrl(maplibreWorkerUrl)

const route = useRoute()
const router = useRouter()

const maps = ref([])
const mapsLoading = ref(false)
const mapsError = ref(null)
const mapError = ref(null)
const selectedMap = ref(null)
const selectedMapBounds = ref(null)
const showSelector = ref(true)
const showLayers = ref(true)
const showInfo = ref(false)
const showTools = ref(false)
const renderMode = ref('unknown')
const layerVisibility = ref(defaultLayerVisibility())

const layerToggles = [
  { group: 'water', label: '💧 Water' },
  { group: 'landuse', label: '🌳 Land Use' },
  { group: 'park', label: '🌲 Parks' },
  { group: 'building', label: '🏢 Buildings' },
  { group: 'road', label: '🛣️ Roads' },
  { group: 'railway', label: '🚂 Railways' },
  { group: 'boundary', label: '🗺️ Boundaries' },
  { group: 'labels', label: '🔤 Labels' }
]

// Saving POI files is admin-gated server-side (PUT /api/poi is protected);
// hide the save/create/edit affordances for guests. Reactive on auth state.
const canEdit = computed(() => !isAdminLocked())

// Tools state
const activeTool = ref('none') // 'none' | 'measure' | 'poi'

// Location tool
const locating = ref(false)
const locationError = ref(null)

// Measure tool
const measurePoints = ref([])
const measureTotalDistance = computed(() => pathLength(measurePoints.value))

// POI tool
const poiFiles = ref([])
const localPoiFiles = ref([]) // imported but not yet saved file names
const selectedPoiFile = ref('')
const poiData = ref(null)
const poiDirty = ref(false)
const poiSaving = ref(false)
const poiMessage = ref(null)
const showNewPoiFile = ref(false)
const newPoiFileName = ref('')
const pendingPoiCoords = ref(null)
const pendingPoiName = ref('')
const poiNameInput = ref(null)
const gpxInput = ref(null)
let loadedPoiFile = ''

const poiFileOptions = computed(() => {
  const saved = poiFiles.value.map((f) => ({ filename: f.filename, unsaved: false }))
  const known = new Set(saved.map((f) => f.filename))
  const local = localPoiFiles.value.filter((name) => !known.has(name)).map((filename) => ({ filename, unsaved: true }))
  return [...saved, ...local]
})

// Coordinates
const COORD_MODE_KEY = 'fyr.maps.coordMode'
const readCoordMode = () => {
  try { return localStorage.getItem(COORD_MODE_KEY) === 'dms' ? 'dms' : 'decimal' } catch { return 'decimal' }
}
const coordMode = ref(readCoordMode())
const cursorLngLat = ref(null)
const centerLngLat = ref(null)
const coordCopied = ref(false)
const gotoInput = ref('')
const gotoError = ref(null)

const displayCoord = computed(() => {
  if (cursorLngLat.value) return { ...cursorLngLat.value, source: 'cursor' }
  if (centerLngLat.value) return { ...centerLngLat.value, source: 'center' }
  return null
})
const displayCoordText = computed(() => (displayCoord.value
  ? formatCoordinate(displayCoord.value.lat, displayCoord.value.lng, coordMode.value)
  : ''))

// Deep link marker: { lat, lng, zoom, label }
const deepLinkTarget = ref(null)
const markerSummary = computed(() => {
  const t = deepLinkTarget.value
  if (!t) return ''
  const coord = formatCoordinate(t.lat, t.lng, coordMode.value)
  return t.label ? `${t.label} (${coord})` : coord
})

// Track recording (app-wide recorder so SPA navigation does not stop it)
const recorder = getTrackRecorder()
const trackState = recorder.state
const finishedTrack = recorder.finished
const trackName = ref('')
const trackTargetFile = ref('')
const trackSaving = ref(false)
const trackMessage = ref(null)
let centerOnNextFix = false

let mapInstance = null
let pmtilesProtocol = null
let locationMarker = null
let deepLinkMarker = null
let clickPopup = null
let lastAppliedLinkKey = null
let routeApplyToken = 0
let pointerFrame = null
let copiedTimer = null
let userMovedMap = false
// True once the current map instance fired 'load'; overlay sources can only be added after that.
let mapReady = false

const locationState = useLocationState()

const isVectorMode = computed(() => renderMode.value === 'vector')
const renderModeLabel = computed(() => {
  if (renderMode.value === 'vector') return 'Vector tiles'
  if (renderMode.value === 'raster') return 'Raster tiles'
  return 'Unknown'
})

const buildMapDataUrl = (filename) => buildMapDataUrlFor(filename, window.location.origin)

const boundsResolver = createBoundsResolver({
  loadPmtilesHeader: (filename) => new PMTiles(buildMapDataUrl(filename)).getHeader(),
  loadMbtilesMetadata: async (filename) => {
    const res = await fetch(mbtilesMetadataUrl(filename))
    if (!res.ok) throw new Error(`metadata ${res.status}`)
    return res.json()
  }
})

const formatBoundsText = (b) => `${formatDecimal(b[1], b[0], 2)} → ${formatDecimal(b[3], b[2], 2)}`

// ── URL / deep links ────────────────────────────────────────────────

const writeQuery = (query) => {
  lastAppliedLinkKey = deepLinkKey(parseMapsQuery(query))
  const current = route.query || {}
  const same = Object.keys(query).length === Object.keys(current).length &&
    Object.entries(query).every(([k, v]) => current[k] === v)
  if (same) return
  router.replace({ query }).catch(() => {})
}

const currentLinkQuery = (map = selectedMap.value) => {
  const t = deepLinkTarget.value
  return buildMapsQuery({
    map: map?.filename,
    lat: t?.lat,
    lng: t?.lng,
    zoom: t ? t.zoom : undefined,
    label: t?.label
  })
}

/**
 * Pick a map for a point: keep `preferred` when it covers the point (or its
 * coverage is unknown), else the most specific map whose bounds contain it,
 * else `preferred`/the first map.
 */
const chooseMapForPoint = async (lng, lat, preferred) => {
  if (preferred) {
    const b = await boundsResolver.getBounds(preferred)
    if (!b || boundsContain(b, lng, lat)) return preferred
  }
  const picked = await boundsResolver.pickMap(maps.value, lng, lat)
  return picked || preferred || maps.value[0] || null
}

const viewDeepLinkTarget = () => {
  const t = deepLinkTarget.value
  if (!mapInstance || !t) return
  mapInstance.jumpTo({ center: [t.lng, t.lat], zoom: t.zoom })
  updateDeepLinkMarker()
}

const applyRouteQuery = async (query) => {
  const parsed = parseMapsQuery(query)
  const key = deepLinkKey(parsed)
  if (key === lastAppliedLinkKey) return
  lastAppliedLinkKey = key
  const token = ++routeApplyToken

  deepLinkTarget.value = parsed.hasPoint
    ? { lat: parsed.lat, lng: parsed.lng, zoom: parsed.zoom ?? DEFAULT_POINT_ZOOM, label: parsed.label }
    : null
  if (!parsed.hasPoint) updateDeepLinkMarker()

  let target = null
  if (parsed.map) {
    target = maps.value.find((m) => m.filename === parsed.map) || null
    if (!target) mapError.value = `Map "${parsed.map}" is not available.`
  }
  if (!target && parsed.hasPoint) {
    target = await chooseMapForPoint(parsed.lng, parsed.lat, selectedMap.value)
    if (token !== routeApplyToken) return
  }
  if (!target) return

  if (target !== selectedMap.value) {
    selectedMap.value = target // map re-initialises and centres on the deep-link target
  } else {
    viewDeepLinkTarget()
  }
  // Make the view shareable: record the chosen map in the URL.
  writeQuery(currentLinkQuery(target))
}

const selectMap = (map) => {
  selectedMap.value = map
  writeQuery(currentLinkQuery(map))
}

const updateDeepLinkMarker = () => {
  if (deepLinkMarker) {
    deepLinkMarker.remove()
    deepLinkMarker = null
  }
  const t = deepLinkTarget.value
  if (!mapInstance || !t) return
  deepLinkMarker = new maplibregl.Marker({ color: '#e63946' }).setLngLat([t.lng, t.lat])
  if (t.label) {
    // setText: labels come from URLs and must never be interpreted as HTML.
    const popup = new maplibregl.Popup({ offset: 30, closeOnClick: false }).setText(t.label)
    deepLinkMarker.setPopup(popup)
  }
  deepLinkMarker.addTo(mapInstance)
  if (t.label) deepLinkMarker.togglePopup()
}

const clearDeepLinkMarker = () => {
  deepLinkTarget.value = null
  updateDeepLinkMarker()
  writeQuery(currentLinkQuery())
}

const goToCoordinate = async () => {
  gotoError.value = null
  const parsed = parseCoordinate(gotoInput.value)
  if (!parsed) {
    gotoError.value = 'Could not read that coordinate. Try "59.33, 18.07" or "59°19\'48"N 18°04\'12"E".'
    return
  }
  const zoom = Math.max(mapInstance ? mapInstance.getZoom() : 0, 13)
  deepLinkTarget.value = { lat: parsed.lat, lng: parsed.lng, zoom, label: null }
  const map = await chooseMapForPoint(parsed.lng, parsed.lat, selectedMap.value)
  if (map && map !== selectedMap.value) {
    selectedMap.value = map
  } else {
    if (mapInstance) mapInstance.flyTo({ center: [parsed.lng, parsed.lat], zoom })
    updateDeepLinkMarker()
  }
  writeQuery(currentLinkQuery(map || selectedMap.value))
}

// ── Coordinates readout and click popup ─────────────────────────────

const toggleCoordMode = () => {
  coordMode.value = coordMode.value === 'dms' ? 'decimal' : 'dms'
  try { localStorage.setItem(COORD_MODE_KEY, coordMode.value) } catch { /* ignore */ }
}

const flashCopied = () => {
  coordCopied.value = true
  if (copiedTimer) clearTimeout(copiedTimer)
  copiedTimer = setTimeout(() => { coordCopied.value = false }, 1500)
}

const copyDisplayedCoordinate = async () => {
  if (!displayCoordText.value) return
  if (await copyText(displayCoordText.value)) flashCopied()
}

const shareUrlFor = (lat, lng, label) => {
  const query = buildMapsQuery({
    map: selectedMap.value?.filename,
    lat,
    lng,
    zoom: mapInstance ? mapInstance.getZoom() : undefined,
    label
  })
  return new URL(router.resolve({ path: '/maps', query }).href, window.location.origin).toString()
}

const featureLabel = (feature) => {
  const name = feature?.properties?.name
  const type = feature?.geometry?.type
  if (type === 'LineString' || type === 'MultiLineString') {
    const lines = type === 'LineString' ? [feature.geometry.coordinates] : feature.geometry.coordinates
    const km = (lines || []).reduce((sum, line) => sum + pathLength(line || []), 0)
    return `${name || '(unnamed)'} · ${formatDistance(km)}`
  }
  return name || '(unnamed)'
}

const featureIcon = (feature) => {
  const type = feature?.geometry?.type || ''
  if (type.includes('LineString')) return '〰️'
  if (type.includes('Polygon')) return '⬠'
  return '📍'
}

const buildPopupContent = (lngLat, feature) => {
  const root = document.createElement('div')
  root.className = 'fyr-map-popup'
  if (feature) {
    const title = document.createElement('strong')
    title.textContent = featureLabel(feature)
    root.appendChild(title)
    const desc = feature.properties?.desc || feature.properties?.description
    if (desc) {
      const p = document.createElement('div')
      p.textContent = String(desc)
      root.appendChild(p)
    }
  }
  const dec = document.createElement('div')
  dec.className = 'fyr-map-popup-coord'
  dec.textContent = formatDecimal(lngLat.lat, lngLat.lng)
  const dms = document.createElement('div')
  dms.className = 'fyr-map-popup-coord'
  dms.textContent = formatDms(lngLat.lat, lngLat.lng)
  root.append(dec, dms)

  const actions = document.createElement('div')
  actions.className = 'fyr-map-popup-actions'
  const mkButton = (text, onClick) => {
    const b = document.createElement('button')
    b.type = 'button'
    b.textContent = text
    b.addEventListener('click', async () => {
      const ok = await onClick()
      b.textContent = ok ? 'Copied ✓' : 'Copy failed'
      setTimeout(() => { b.textContent = text }, 1500)
    })
    return b
  }
  actions.append(
    mkButton('Copy', () => copyText(formatCoordinate(lngLat.lat, lngLat.lng, coordMode.value))),
    mkButton('Copy link', () => copyText(shareUrlFor(lngLat.lat, lngLat.lng, feature?.properties?.name)))
  )
  root.appendChild(actions)
  return root
}

const showPointPopup = (e) => {
  if (!mapInstance) return
  const layers = POI_INTERACTIVE_LAYER_IDS.filter((id) => mapInstance.getLayer(id))
  const hits = layers.length ? mapInstance.queryRenderedFeatures(e.point, { layers }) : []
  if (clickPopup) clickPopup.remove()
  clickPopup = new maplibregl.Popup({ offset: 8, maxWidth: '260px' })
    .setLngLat(e.lngLat)
    .setDOMContent(buildPopupContent(e.lngLat, hits[0] || null))
    .addTo(mapInstance)
}

const onPointerMove = (e) => {
  if (pointerFrame) return
  pointerFrame = requestAnimationFrame(() => {
    pointerFrame = null
    cursorLngLat.value = { lng: e.lngLat.lng, lat: e.lngLat.lat }
  })
}

const onPointerOut = () => {
  if (pointerFrame) cancelAnimationFrame(pointerFrame)
  pointerFrame = null
  cursorLngLat.value = null
}

const onMapMove = () => {
  if (!mapInstance) return
  const c = mapInstance.getCenter()
  centerLngLat.value = { lng: c.lng, lat: c.lat }
}

const onMapMoveStart = (e) => {
  if (e?.originalEvent) userMovedMap = true
}

// ── Map helpers ─────────────────────────────────────────────────────

const getInitialView = () => {
  const t = deepLinkTarget.value
  if (t) return { center: [t.lng, t.lat], zoom: t.zoom }
  const location = locationState.location
  if (location) return { center: [location.longitude, location.latitude], zoom: 6 }
  return { center: [13.4, 59.5], zoom: 6 }
}

const updateLocationMarker = (location) => {
  if (locationMarker) {
    locationMarker.remove()
    locationMarker = null
  }

  if (!mapInstance || !location) return

  locationMarker = new maplibregl.Marker({ color: '#7b5cff' })
    .setLngLat([location.longitude, location.latitude])
    .addTo(mapInstance)
}

const setGroupVisibility = (groupName, isVisible) => {
  const ids = layerGroups[groupName] || []
  for (const layerId of ids) {
    if (!mapInstance || !mapInstance.getLayer(layerId)) continue
    mapInstance.setLayoutProperty(layerId, 'visibility', isVisible ? 'visible' : 'none')
  }
}

const toggleLayer = (groupName) => {
  if (!mapInstance || !isVectorMode.value) return
  setGroupVisibility(groupName, layerVisibility.value[groupName])
}

const updateCursor = () => {
  if (!mapInstance) return
  const placing = activeTool.value === 'measure' ||
    (activeTool.value === 'poi' && canEdit.value && Boolean(selectedPoiFile.value && poiData.value))
  mapInstance.getCanvas().style.cursor = placing ? 'crosshair' : ''
}

const fitFeatures = (features) => {
  if (!mapInstance) return
  const bounds = new maplibregl.LngLatBounds()
  let any = false
  const visit = (c) => {
    if (!Array.isArray(c)) return
    if (typeof c[0] === 'number' && typeof c[1] === 'number') {
      bounds.extend([c[0], c[1]])
      any = true
      return
    }
    c.forEach(visit)
  }
  for (const f of features || []) visit(f?.geometry?.coordinates)
  if (!any) return
  mapInstance.fitBounds(bounds, { padding: 60, maxZoom: 15, duration: 600 })
}

const zoomToFeature = (feature) => fitFeatures([feature])

// ── Center on user location (issue #43) ────────────────────────────
const centerOnUserLocation = () => {
  if (!mapInstance) return
  const support = checkGeolocationSupport({ isSecureContext: window.isSecureContext, geolocation: navigator.geolocation })
  if (!support.ok) {
    locationError.value = support.message
    return
  }
  locating.value = true
  locationError.value = null
  navigator.geolocation.getCurrentPosition(
    (pos) => {
      locating.value = false
      const { longitude, latitude } = pos.coords
      mapInstance.flyTo({ center: [longitude, latitude], zoom: 14 })
      updateLocationMarker({ longitude, latitude })
    },
    (err) => {
      locating.value = false
      locationError.value = geolocationErrorMessage(err, { isSecureContext: window.isSecureContext })
    },
    { timeout: 10000 }
  )
}

// ── Measure tool (issue #41) ────────────────────────────────────────
const getMeasureGeoJSON = () => buildMeasureGeoJSON(measurePoints.value)

const addMeasureLayers = () => {
  if (!mapInstance || !mapReady || mapInstance.getSource(MEASURE_SOURCE_ID)) return
  mapInstance.addSource(MEASURE_SOURCE_ID, { type: 'geojson', data: getMeasureGeoJSON() })
  for (const layer of buildMeasureLayers()) mapInstance.addLayer(layer)
}

const updateMeasureSource = () => {
  if (!mapInstance) return
  const src = mapInstance.getSource(MEASURE_SOURCE_ID)
  if (src) src.setData(getMeasureGeoJSON())
}

const onMeasureClick = (e) => {
  measurePoints.value = [...measurePoints.value, [e.lngLat.lng, e.lngLat.lat]]
  updateMeasureSource()
}

const clearMeasure = () => {
  measurePoints.value = []
  updateMeasureSource()
}

// ── Track recording ─────────────────────────────────────────────────
const setTrackData = (track) => {
  if (!mapInstance || !mapReady) return
  const data = trackToGeoJSON(track)
  const src = mapInstance.getSource(TRACK_SOURCE_ID)
  if (src) {
    src.setData(data)
    return
  }
  if (!track) return
  mapInstance.addSource(TRACK_SOURCE_ID, { type: 'geojson', data })
  for (const layer of buildTrackLayers()) mapInstance.addLayer(layer)
}

const onTrackFix = (fix) => {
  updateLocationMarker({ longitude: fix.lng, latitude: fix.lat })
  if (centerOnNextFix && mapInstance) {
    centerOnNextFix = false
    mapInstance.easeTo({ center: [fix.lng, fix.lat], zoom: Math.max(mapInstance.getZoom(), 14) })
  }
}

const toggleRecording = () => {
  trackMessage.value = null
  if (trackState.recording) {
    recorder.stop()
    return
  }
  if (finishedTrack.value && !window.confirm('Start a new recording? The current unsaved track will be discarded.')) return
  trackName.value = ''
  centerOnNextFix = true
  recorder.start()
}

const discardTrack = () => {
  if (!window.confirm('Discard the recorded track?')) return
  recorder.discard()
  trackName.value = ''
  trackMessage.value = null
}

const currentTrackFeature = () => trackToFeature(recorder.current(), trackName.value.trim())

const downloadTrackGpx = () => {
  const feature = currentTrackFeature()
  if (!feature) return
  const { xml } = toGpx({ type: 'FeatureCollection', features: [feature] }, { name: feature.properties.name })
  downloadText(`${slugifyFilename(feature.properties.name, 'track')}.gpx`, xml, 'application/gpx+xml')
}

const isNotFound = (err) => err?.response?.status === 404

const saveTrack = async () => {
  const feature = currentTrackFeature()
  if (!feature) return
  const target = normalizePoiFilename(trackTargetFile.value || 'tracks.geojson')
  trackSaving.value = true
  trackMessage.value = null
  try {
    if (target === selectedPoiFile.value && poiData.value) {
      poiData.value.features.push(feature)
      poiDirty.value = true
      addPoiLayersToMap()
      if (!(await savePoiFile())) {
        poiData.value.features.pop()
        addPoiLayersToMap()
        throw new Error(poiMessage.value?.text || 'Saving failed')
      }
    } else {
      let data = emptyGeoJSON()
      try {
        data = normalizePoiData(await apiService.readPoi(target))
      } catch (err) {
        if (!isNotFound(err)) throw err
      }
      data.features.push(feature)
      await apiService.savePoi(target, data)
      await loadPoiFiles()
      if (!selectedPoiFile.value) {
        selectedPoiFile.value = target
        loadedPoiFile = target
        poiData.value = data
        poiDirty.value = false
        addPoiLayersToMap()
      }
    }
    recorder.discard()
    trackName.value = ''
    trackMessage.value = { type: 'success', text: `Track saved to ${target}.` }
  } catch (err) {
    console.error('Error saving track:', err)
    trackMessage.value = { type: 'error', text: `Could not save track: ${err?.response ? apiService.handleError(err) : err.message}` }
  } finally {
    trackSaving.value = false
  }
}

// ── POI tool (issue #42) ────────────────────────────────────────────
const addPoiLayersToMap = () => {
  if (!mapInstance || !mapReady) return
  const data = poiData.value || emptyGeoJSON()
  const existing = mapInstance.getSource(POI_SOURCE_ID)
  if (existing) {
    existing.setData(data)
    return
  }
  mapInstance.addSource(POI_SOURCE_ID, { type: 'geojson', data })
  for (const layer of buildPoiLayers()) mapInstance.addLayer(layer)
}

const removePoiLayersFromMap = () => {
  if (!mapInstance) return
  for (const id of poiLayerIds()) {
    if (mapInstance.getLayer(id)) mapInstance.removeLayer(id)
  }
  if (mapInstance.getSource(POI_SOURCE_ID)) mapInstance.removeSource(POI_SOURCE_ID)
}

const loadPoiFiles = async () => {
  try {
    const response = await apiService.getPOIs()
    poiFiles.value = response.data || []
  } catch (err) {
    console.error('Error loading POI file list:', err)
  }
}

const onPoiFileChange = async () => {
  if (poiDirty.value && loadedPoiFile && selectedPoiFile.value !== loadedPoiFile &&
      !window.confirm(`Discard unsaved changes to ${loadedPoiFile}?`)) {
    selectedPoiFile.value = loadedPoiFile
    return
  }
  poiMessage.value = null
  pendingPoiCoords.value = null
  if (loadedPoiFile && localPoiFiles.value.includes(loadedPoiFile) && loadedPoiFile !== selectedPoiFile.value) {
    // Unsaved imported data is dropped when switching away.
    localPoiFiles.value = localPoiFiles.value.filter((n) => n !== loadedPoiFile)
  }
  if (!selectedPoiFile.value) {
    loadedPoiFile = ''
    poiData.value = null
    poiDirty.value = false
    removePoiLayersFromMap()
    updateCursor()
    return
  }
  const requestedFile = selectedPoiFile.value
  try {
    const data = await apiService.readPoi(requestedFile)
    if (selectedPoiFile.value !== requestedFile) return
    poiData.value = normalizePoiData(data)
  } catch (err) {
    if (selectedPoiFile.value !== requestedFile) return
    console.error('Error loading POI file:', err)
    poiData.value = emptyGeoJSON()
  }
  loadedPoiFile = requestedFile
  poiDirty.value = false
  addPoiLayersToMap()
  updateCursor()
}

const createPoiFile = async () => {
  const filename = normalizePoiFilename(newPoiFileName.value)
  if (!filename) return
  const newData = emptyGeoJSON()
  try {
    await apiService.savePoi(filename, newData)
    await loadPoiFiles()
    selectedPoiFile.value = filename
    loadedPoiFile = filename
    poiData.value = newData
    poiDirty.value = false
    showNewPoiFile.value = false
    newPoiFileName.value = ''
    poiMessage.value = null
    addPoiLayersToMap()
    updateCursor()
  } catch (err) {
    console.error('Error creating POI file:', err)
    poiMessage.value = { type: 'error', text: `Could not create file: ${apiService.handleError(err)}` }
  }
}

const onPoiClick = (e) => {
  if (!canEdit.value || !selectedPoiFile.value || !poiData.value) return
  if (pendingPoiCoords.value) return
  pendingPoiCoords.value = [e.lngLat.lng, e.lngLat.lat]
  pendingPoiName.value = ''
  nextTick(() => { poiNameInput.value?.focus() })
}

const confirmPoi = () => {
  if (!pendingPoiCoords.value || !pendingPoiName.value.trim()) return
  if (!poiData.value) poiData.value = emptyGeoJSON()
  poiData.value.features.push(makePointFeature(pendingPoiCoords.value, pendingPoiName.value.trim()))
  poiDirty.value = true
  addPoiLayersToMap()
  pendingPoiCoords.value = null
  pendingPoiName.value = ''
}

const cancelPoi = () => {
  pendingPoiCoords.value = null
  pendingPoiName.value = ''
}

const removePoi = (index) => {
  if (!poiData.value) return
  poiData.value.features.splice(index, 1)
  poiDirty.value = true
  addPoiLayersToMap()
}

const savePoiFile = async () => {
  if (!selectedPoiFile.value || !poiData.value) return false
  poiSaving.value = true
  try {
    await apiService.savePoi(selectedPoiFile.value, poiData.value)
    poiDirty.value = false
    poiMessage.value = null
    if (localPoiFiles.value.includes(selectedPoiFile.value)) {
      localPoiFiles.value = localPoiFiles.value.filter((n) => n !== selectedPoiFile.value)
      await loadPoiFiles()
    }
    return true
  } catch (err) {
    console.error('Error saving POI file:', err)
    poiMessage.value = { type: 'error', text: `Could not save: ${apiService.handleError(err)}` }
    return false
  } finally {
    poiSaving.value = false
  }
}

// ── GPX import / export ─────────────────────────────────────────────
const openGpxPicker = () => {
  poiMessage.value = null
  gpxInput.value?.click()
}

const describeImport = (counts) => {
  const parts = []
  if (counts.waypoints) parts.push(`${counts.waypoints} waypoint${counts.waypoints !== 1 ? 's' : ''}`)
  if (counts.tracks) parts.push(`${counts.tracks} track${counts.tracks !== 1 ? 's' : ''}`)
  if (counts.routes) parts.push(`${counts.routes} route${counts.routes !== 1 ? 's' : ''}`)
  let text = parts.length ? `Imported ${parts.join(', ')}.` : 'The GPX file contained no waypoints, tracks or routes.'
  if (counts.skippedPoints) text += ` Skipped ${counts.skippedPoints} invalid point${counts.skippedPoints !== 1 ? 's' : ''}.`
  return text
}

const onGpxFileSelected = async (event) => {
  const input = event.target
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  let result
  try {
    result = parseGpx(await readFileText(file))
  } catch (err) {
    poiMessage.value = { type: 'error', text: `Could not import ${file.name}: ${err.message}` }
    return
  }
  if (result.features.length === 0) {
    poiMessage.value = { type: 'error', text: describeImport(result.counts) }
    return
  }

  if (!selectedPoiFile.value || !poiData.value) {
    // Import into a new, not yet saved file named after the GPX.
    const existing = poiFileOptions.value.map((f) => f.filename)
    const filename = uniquePoiFilename(result.name || file.name, existing)
    localPoiFiles.value = [...localPoiFiles.value, filename]
    selectedPoiFile.value = filename
    loadedPoiFile = filename
    poiData.value = emptyGeoJSON()
  }
  poiData.value.features.push(...result.features)
  poiDirty.value = true
  pendingPoiCoords.value = null
  addPoiLayersToMap()
  updateCursor()
  fitFeatures(result.features)
  const saveHint = canEdit.value ? ' Click "Save Changes" to keep it.' : ''
  poiMessage.value = { type: 'success', text: describeImport(result.counts) + saveHint }
}

const exportBaseName = () => slugifyFilename(selectedPoiFile.value || 'pois', 'pois')

const exportPoiGpx = () => {
  if (!poiData.value) return
  const { xml, exported, skipped } = toGpx(poiData.value, { name: selectedPoiFile.value || 'Fyr POIs' })
  downloadText(`${exportBaseName()}.gpx`, xml, 'application/gpx+xml')
  poiMessage.value = skipped
    ? { type: 'success', text: `Exported ${exported} feature(s); ${skipped} polygon/other feature(s) cannot be represented in GPX.` }
    : null
}

const exportPoiGeoJson = () => {
  if (!poiData.value) return
  downloadText(`${exportBaseName()}.geojson`, JSON.stringify(poiData.value, null, 2), 'application/geo+json')
}

// ── Tool activation / deactivation ─────────────────────────────────
const onMapClick = (e) => {
  if (activeTool.value === 'measure') {
    onMeasureClick(e)
  } else if (activeTool.value === 'poi' && canEdit.value && selectedPoiFile.value && poiData.value) {
    onPoiClick(e)
  } else {
    showPointPopup(e)
  }
}

const activateTool = (tool) => {
  if (tool === 'measure') {
    addMeasureLayers()
  } else if (tool === 'poi') {
    loadPoiFiles()
  }
  updateCursor()
}

const deactivateTool = (tool) => {
  if (tool === 'poi' && pendingPoiCoords.value) cancelPoi()
  updateCursor()
}

const toggleTool = (tool) => {
  if (clickPopup) clickPopup.remove()
  if (activeTool.value === tool) {
    activeTool.value = 'none'
    deactivateTool(tool)
  } else {
    const previous = activeTool.value
    activeTool.value = tool
    deactivateTool(previous)
    activateTool(tool)
  }
}

// Re-attach overlay layers after a map re-initialization
const afterMapLoaded = () => {
  if (activeTool.value === 'measure') addMeasureLayers()
  if (poiData.value) addPoiLayersToMap()
  if (recorder.current()) setTrackData(recorder.current())
  updateCursor()
}

const fitToMapBoundsIfUseful = async (map) => {
  const bounds = await boundsResolver.getBounds(map)
  if (selectedMap.value !== map) return
  selectedMapBounds.value = bounds
  if (!bounds || !mapInstance || deepLinkTarget.value || userMovedMap) return
  const loc = locationState.location
  if (loc && boundsContain(bounds, loc.longitude, loc.latitude)) return
  const [w, s, e, n] = bounds
  // Antimeridian-crossing bounds: MapLibre expects east > west.
  mapInstance.fitBounds([[w, s], [e < w ? e + 360 : e, n]], { padding: 20, duration: 0 })
}

const loadMaps = async () => {
  mapsLoading.value = true
  mapsError.value = null
  try {
    const response = await apiService.getMaps()
    maps.value = response.data || []
  } catch (err) {
    console.error('Error loading maps:', err)
    mapsError.value = apiService.handleError(err)
  } finally {
    mapsLoading.value = false
  }
}

const initializeMap = async () => {
  mapError.value = null
  try {
    cleanupMap()

    const mapContainer = document.getElementById('map')
    if (!mapContainer || !selectedMap.value) {
      return
    }

    const view = getInitialView()
    userMovedMap = false
    selectedMapBounds.value = null
    const map = new maplibregl.Map({
      container: mapContainer,
      style: buildBaseStyle(),
      center: view.center,
      zoom: view.zoom,
      pitch: 0,
      bearing: 0
    })
    mapInstance = map

    map.addControl(new maplibregl.NavigationControl(), 'top-right')
    map.addControl(new maplibregl.ScaleControl({ unit: 'metric' }), 'bottom-right')

    map.on('click', onMapClick)
    map.on('mousemove', onPointerMove)
    map.on('mouseout', onPointerOut)
    map.on('move', onMapMove)
    map.on('movestart', onMapMoveStart)
    onMapMove()

    map.on('load', async () => {
      try {
        const currentMap = selectedMap.value
        const mode = await loadMapSource(map, currentMap.filename, { origin: window.location.origin })
        // The user may have switched maps while the archive header was loading.
        if (mapInstance !== map) return

        if (mode) renderMode.value = mode
        if (mode === 'vector') {
          for (const groupName of Object.keys(layerGroups)) {
            setGroupVisibility(groupName, layerVisibility.value[groupName])
          }
        }

        if (!mode) {
          renderMode.value = 'unknown'
          mapError.value = 'Unable to render the selected map.'
          console.warn(`Unable to load map source for ${currentMap.filename}`)
        }

        mapReady = true
        updateLocationMarker(locationState.location)
        updateDeepLinkMarker()
        afterMapLoaded()
        fitToMapBoundsIfUseful(currentMap)
      } catch (error) {
        renderMode.value = 'unknown'
        mapError.value = 'Failed to load map data.'
        console.error('Map load error:', error.message)
      }
    })

    map.on('error', (event) => {
      mapError.value = 'Map renderer reported an error.'
      console.error('Map error:', event?.error || event)
    })
  } catch (error) {
    renderMode.value = 'unknown'
    mapError.value = 'Map initialization failed.'
    console.error('Error initializing map:', error)
  }
}

const cleanupMap = () => {
  mapReady = false
  if (clickPopup) {
    clickPopup.remove()
    clickPopup = null
  }
  if (deepLinkMarker) {
    deepLinkMarker.remove()
    deepLinkMarker = null
  }
  if (mapInstance) {
    mapInstance.off('click', onMapClick)
    mapInstance.off('mousemove', onPointerMove)
    mapInstance.off('mouseout', onPointerOut)
    mapInstance.off('move', onMapMove)
    mapInstance.off('movestart', onMapMoveStart)
    mapInstance.remove()
    mapInstance = null
  }
  cursorLngLat.value = null
}

watch(selectedMap, (newMap) => {
  if (newMap) {
    // Reset per-map tool state when switching maps
    if (activeTool.value === 'measure') {
      measurePoints.value = []
    }
    if (activeTool.value === 'poi') {
      pendingPoiCoords.value = null
      pendingPoiName.value = ''
    }
    setTimeout(initializeMap, 120)
  } else {
    renderMode.value = 'unknown'
    cleanupMap()
  }
})

watch(() => locationState.location, (location) => {
  if (!mapInstance) return
  // Only update the marker; don't auto-pan so the user isn't disrupted while browsing
  updateLocationMarker(location)
}, { deep: true })

watch(() => route.query, (query) => {
  if (mapsLoading.value) return
  applyRouteQuery(query)
})

watch(canEdit, (editable) => {
  if (!editable && pendingPoiCoords.value) cancelPoi()
  updateCursor()
})

watch(finishedTrack, (track) => {
  if (track && !trackName.value) trackName.value = defaultTrackName(new Date(track.startedAt))
  if (track && !trackTargetFile.value) trackTargetFile.value = selectedPoiFile.value || 'tracks.geojson'
}, { immediate: true })

onMounted(async () => {
  if (!pmtilesProtocol) {
    pmtilesProtocol = new Protocol()
    maplibregl.addProtocol('pmtiles', pmtilesProtocol.tile)
  }

  recorder.setListeners({ onChange: setTrackData, onFix: onTrackFix })
  if (!trackState.recording && !finishedTrack.value) recorder.recover()

  await loadMaps()
  await applyRouteQuery(route.query)
})

onBeforeUnmount(() => {
  recorder.setListeners({})
  if (copiedTimer) clearTimeout(copiedTimer)
  if (pointerFrame) cancelAnimationFrame(pointerFrame)
  if (locationMarker) {
    locationMarker.remove()
    locationMarker = null
  }
  cleanupMap()
})
</script>

<style scoped>
.maps-page {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  min-height: calc(100vh - 120px);
}

.description {
  color: #b0b0b0;
  margin: 0;
}

.maps-container {
  position: relative;
  width: 100%;
  min-height: 640px;
  height: calc(100vh - 240px);
  border-radius: 10px;
  overflow: hidden;
  background: #1a1a1a;
  border: 1px solid #3a3a3a;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
}

.map-canvas {
  width: 100%;
  height: 100%;
}

.overlay {
  position: absolute;
  z-index: 10;
  background: rgba(35, 35, 35, 0.93);
  border: 1px solid #4a4a4a;
  border-radius: 8px;
  box-shadow: 0 8px 18px rgba(0, 0, 0, 0.35);
  backdrop-filter: blur(6px);
  max-height: calc(100% - 1.5rem);
  overflow: visible;
}

.overlay-toggle {
  position: absolute;
  top: 0.6rem;
  width: 32px;
  height: 32px;
  border: none;
  border-radius: 6px;
  background: #667eea;
  color: #ffffff;
  cursor: pointer;
  font-size: 1rem;
  z-index: 11;
}

.overlay-content {
  padding: 0.85rem;
  padding-top: 2.8rem;
  max-height: calc(100vh - 220px);
  overflow: auto;
}

.overlay.collapsed {
  width: 46px !important;
  height: 46px;
  min-width: 46px;
  min-height: 46px;
  border: 1px solid #4a4a4a;
  border-radius: 8px;
  background: rgba(35, 35, 35, 0.95);
}

.overlay.collapsed .overlay-toggle {
  top: 6px;
  left: 6px;
  right: auto;
}

.overlay-content h3 {
  margin: 0 0 0.6rem 0;
  color: #e0e0e0;
  font-size: 0.95rem;
}

.overlay-meta {
  margin: 0.35rem 0;
  font-size: 0.82rem;
  color: #bdbdbd;
  line-height: 1.35;
}

.overlay-selector {
  top: 0.75rem;
  left: 0.75rem;
  width: 300px;
}

.overlay-selector .overlay-toggle {
  right: 0.6rem;
}

.overlay-layers {
  top: 7rem;
  right: 0.75rem;
  width: 250px;
}

.overlay-layers .overlay-toggle {
  right: 0.6rem;
}

.overlay-info {
  bottom: 2.4rem;
  left: 0.75rem;
  width: 300px;
}

.overlay-info .overlay-toggle {
  right: 0.6rem;
}

.overlay-tools {
  bottom: 2.4rem;
  right: 0.75rem;
  width: 260px;
}

.overlay-tools .overlay-toggle {
  right: 0.6rem;
}

/* The tools panel is anchored to the bottom; keep it inside the map container
   (container height is 100vh - 240px, panel sits 2.4rem above its bottom). */
.overlay-tools .overlay-content {
  max-height: max(12rem, calc(100vh - 240px - 3.4rem));
}

/* Tool button styles */
.tool-group {
  margin-bottom: 0.75rem;
}

.tool-group:last-child {
  margin-bottom: 0;
}

.tool-btn {
  display: block;
  width: 100%;
  padding: 0.45rem 0.7rem;
  background: #2a2a2a;
  border: 1px solid #4a4a4a;
  border-radius: 5px;
  color: #e0e0e0;
  font-size: 0.88rem;
  text-align: left;
  cursor: pointer;
  transition: all 0.18s;
}

.tool-btn:hover:not(:disabled) {
  background: #333;
  border-color: #667eea;
}

.tool-btn.active {
  background: #2d3f5a;
  border-color: #667eea;
  color: #8ea2ff;
}

.tool-btn.saved {
  color: #7ec98c;
  border-color: #4a6a4f;
}

.tool-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.tool-btn.loading {
  opacity: 0.7;
  cursor: wait;
}

.tool-btn-secondary {
  padding: 0.35rem 0.55rem;
  background: #252525;
  border: 1px solid #4a4a4a;
  border-radius: 4px;
  color: #c0c0c0;
  font-size: 0.82rem;
  cursor: pointer;
  transition: all 0.15s;
}

.tool-btn-secondary:hover:not(:disabled) {
  background: #333;
  border-color: #667eea;
  color: #e0e0e0;
}

.tool-btn-secondary:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.tool-btn-icon {
  padding: 0.3rem 0.5rem;
  background: #252525;
  border: 1px solid #4a4a4a;
  border-radius: 4px;
  color: #c0c0c0;
  font-size: 0.82rem;
  cursor: pointer;
  line-height: 1;
  transition: all 0.15s;
}

.tool-btn-icon:hover {
  background: #333;
  border-color: #667eea;
  color: #e0e0e0;
}

.tool-btn-icon.active {
  background: #2d3f5a;
  border-color: #667eea;
  color: #8ea2ff;
}

.tool-sub-panel {
  margin-top: 0.5rem;
  padding: 0.55rem 0.6rem;
  background: rgba(0, 0, 0, 0.25);
  border-radius: 5px;
  border: 1px solid #3a3a3a;
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.tool-hint {
  margin: 0;
  font-size: 0.78rem;
  color: #999;
  line-height: 1.35;
}

.tool-dim {
  font-size: 0.8rem;
  color: #666;
  font-style: italic;
}

.tool-error {
  margin: 0.3rem 0 0;
  font-size: 0.78rem;
  color: #ff9090;
}

.measure-info {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 0.85rem;
  color: #d0d0d0;
}

.measure-distance {
  font-weight: 600;
  color: #f7c948;
}

/* POI tool sub-elements */
.poi-file-row {
  display: flex;
  gap: 0.4rem;
  align-items: center;
}

.poi-select {
  flex: 1;
  background: #1e1e1e;
  border: 1px solid #4a4a4a;
  border-radius: 4px;
  color: #e0e0e0;
  padding: 0.3rem 0.4rem;
  font-size: 0.8rem;
}

.poi-create-row {
  display: flex;
  gap: 0.4rem;
  align-items: center;
}

.poi-name-row {
  display: flex;
  gap: 0.4rem;
  align-items: center;
}

.poi-input {
  flex: 1;
  background: #1e1e1e;
  border: 1px solid #4a4a4a;
  border-radius: 4px;
  color: #e0e0e0;
  padding: 0.3rem 0.5rem;
  font-size: 0.82rem;
}

.poi-input:focus {
  outline: none;
  border-color: #667eea;
}

.poi-list {
  max-height: 120px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.poi-list-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.25rem 0.4rem;
  background: rgba(255, 255, 255, 0.04);
  border-radius: 3px;
  font-size: 0.8rem;
  color: #c0c0c0;
}

.poi-list-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.poi-remove-btn {
  background: none;
  border: none;
  color: #888;
  cursor: pointer;
  padding: 0 0.2rem;
  font-size: 0.75rem;
  flex-shrink: 0;
}

.poi-remove-btn:hover {
  color: #ff7070;
}

.poi-save-row {
  display: flex;
  justify-content: flex-end;
}

.maps-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.map-item {
  padding: 0.7rem;
  background: #1a1a1a;
  border: 2px solid #3a3a3a;
  border-radius: 4px;
  cursor: pointer;
  transition: all 0.2s;
  text-align: left;
  display: flex;
  justify-content: space-between;
  align-items: center;
  color: #e0e0e0;
}

.map-item:hover {
  background: #252525;
  border-color: #667eea;
}

.map-item.active {
  background: #2d3f5a;
  border-color: #667eea;
}

.map-name {
  font-weight: 600;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}

.map-size {
  font-size: 0.82rem;
  color: #9a9a9a;
  margin-left: 0.6rem;
  white-space: nowrap;
}

.layer-toggles {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.layer-toggle {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  padding: 0.35rem 0.5rem;
  border-radius: 4px;
  color: #e0e0e0;
}

.layer-toggle:hover {
  background: rgba(102, 126, 234, 0.14);
}

.layer-toggle.disabled {
  opacity: 0.5;
}

.layer-toggle input {
  accent-color: #667eea;
}

.layer-name {
  font-size: 0.9rem;
}

.empty-state {
  color: #8c8c8c;
  font-style: italic;
  text-align: center;
  padding: 0.8rem;
}

.empty-state a {
  color: #8ea2ff;
  text-decoration: none;
}

.status-state,
.error-state {
  padding: 0.7rem;
  border-radius: 6px;
  font-size: 0.85rem;
}

.status-state {
  background: rgba(102, 126, 234, 0.18);
  border: 1px solid rgba(142, 162, 255, 0.4);
  color: #dbe2ff;
}

.error-state {
  background: rgba(164, 45, 45, 0.22);
  border: 1px solid rgba(220, 112, 112, 0.5);
  color: #ffd3d3;
}

.empty-view {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #9d9d9d;
  font-size: 1.1rem;
  background: rgba(15, 15, 15, 0.45);
  z-index: 6;
}

.map-note {
  position: absolute;
  left: 0.8rem;
  right: 0.8rem;
  bottom: 0.6rem;
  margin: 0;
  z-index: 7;
  color: #c7c7c7;
  background: rgba(18, 18, 18, 0.65);
  border: 1px solid #3c3c3c;
  border-radius: 6px;
  padding: 0.45rem 0.6rem;
  font-size: 0.8rem;
}

.map-error-banner {
  position: absolute;
  top: 0.8rem;
  left: 50%;
  transform: translateX(-50%);
  z-index: 12;
  background: rgba(136, 28, 28, 0.9);
  border: 1px solid rgba(248, 150, 150, 0.8);
  color: #ffe6e6;
  border-radius: 6px;
  padding: 0.4rem 0.7rem;
  font-size: 0.82rem;
}

/* Coordinate readout in the bottom status bar */
.map-note {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.coord-readout,
.coord-mode-btn {
  background: rgba(0, 0, 0, 0.35);
  border: 1px solid #4a4a4a;
  border-radius: 4px;
  color: #e8e8e8;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 0.76rem;
  line-height: 1.2;
  padding: 0 0.4rem;
  cursor: pointer;
  white-space: nowrap;
}

.coord-readout:hover,
.coord-mode-btn:hover {
  border-color: #667eea;
}

.coord-source {
  opacity: 0.7;
  margin-right: 0.2rem;
}

.coord-copied {
  color: #7ec98c;
  margin-left: 0.35rem;
  font-family: inherit;
}

.map-note-mode {
  flex: 1;
  min-width: 0;
}

.goto-row,
.marker-row,
.tool-row {
  display: flex;
  gap: 0.4rem;
  align-items: center;
  flex-wrap: wrap;
}

.goto-row .poi-input {
  min-width: 0;
}

.marker-row {
  margin-top: 0.35rem;
  justify-content: space-between;
  flex-wrap: nowrap;
}

.marker-row .tool-hint {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tool-warning {
  margin: 0.3rem 0 0;
  font-size: 0.78rem;
  color: #f7c948;
}

.tool-success {
  margin: 0.3rem 0 0;
  font-size: 0.78rem;
  color: #7ec98c;
}

.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
  white-space: nowrap;
}

button.poi-list-name {
  background: none;
  border: none;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
  padding: 0;
  flex: 1;
  min-width: 0;
}

button.poi-list-name:hover {
  color: #8ea2ff;
}

@media (max-width: 1024px) {
  .maps-container {
    min-height: 560px;
    height: calc(100vh - 220px);
  }

  .overlay-selector,
  .overlay-info {
    width: 260px;
  }

  .overlay-layers,
  .overlay-tools {
    width: 230px;
  }
}

@media (max-width: 768px) {
  .maps-container {
    min-height: 500px;
    height: calc(100vh - 190px);
  }

  .overlay {
    max-width: calc(100% - 1rem);
  }

  .overlay-selector,
  .overlay-layers {
    width: auto;
    left: 0.5rem;
    right: 0.5rem;
  }

  .overlay-selector {
    top: 0.5rem;
  }

  .overlay-layers {
    top: 7rem;
  }

  /* On mobile, info and tools sit side-by-side at the bottom (left vs right)
     to avoid the collapsed toggles (46 px tall) overlapping each other.
     When expanded they grow to full width like the other overlays. */
  .overlay-info {
    bottom: 2.2rem;
    left: 0.5rem;
    width: auto;
  }

  .overlay-info:not(.collapsed) {
    right: 0.5rem;
  }

  .overlay-tools {
    bottom: 2.2rem;
    right: 0.5rem;
    width: auto;
  }

  .overlay-tools:not(.collapsed) {
    left: 0.5rem;
  }

  .overlay-content {
    padding: 0.75rem;
    padding-top: 2.7rem;
  }

  .map-note {
    font-size: 0.76rem;
    padding: 0.2rem 0.45rem;
  }

  /* Keep the status bar to one line so it never grows under the overlays. */
  .map-note-mode {
    display: none;
  }
}
</style>

<style>
/* MapLibre popups are rendered outside this component, so these are global. */
.fyr-map-popup {
  font-size: 0.82rem;
  color: #222;
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
}

.fyr-map-popup-coord {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 0.78rem;
}

.fyr-map-popup-actions {
  display: flex;
  gap: 0.35rem;
  margin-top: 0.3rem;
}

.fyr-map-popup-actions button {
  border: 1px solid #667eea;
  background: #eef0ff;
  color: #2d3f8a;
  border-radius: 4px;
  padding: 0.15rem 0.45rem;
  font-size: 0.78rem;
  cursor: pointer;
}
</style>
