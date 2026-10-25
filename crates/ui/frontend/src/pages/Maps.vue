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
            <label class="layer-toggle" :class="{ disabled: !isVectorMode }">
              <input
                type="checkbox"
                :disabled="!isVectorMode"
                v-model="layerVisibility.water"
                @change="toggleLayer('water')"
              >
              <span class="layer-name">💧 Water</span>
            </label>
            <label class="layer-toggle" :class="{ disabled: !isVectorMode }">
              <input
                type="checkbox"
                :disabled="!isVectorMode"
                v-model="layerVisibility.landuse"
                @change="toggleLayer('landuse')"
              >
              <span class="layer-name">🌳 Land Use</span>
            </label>
            <label class="layer-toggle" :class="{ disabled: !isVectorMode }">
              <input
                type="checkbox"
                :disabled="!isVectorMode"
                v-model="layerVisibility.park"
                @change="toggleLayer('park')"
              >
              <span class="layer-name">🌲 Parks</span>
            </label>
            <label class="layer-toggle" :class="{ disabled: !isVectorMode }">
              <input
                type="checkbox"
                :disabled="!isVectorMode"
                v-model="layerVisibility.building"
                @change="toggleLayer('building')"
              >
              <span class="layer-name">🏢 Buildings</span>
            </label>
            <label class="layer-toggle" :class="{ disabled: !isVectorMode }">
              <input
                type="checkbox"
                :disabled="!isVectorMode"
                v-model="layerVisibility.road"
                @change="toggleLayer('road')"
              >
              <span class="layer-name">🛣️ Roads</span>
            </label>
            <label class="layer-toggle" :class="{ disabled: !isVectorMode }">
              <input
                type="checkbox"
                :disabled="!isVectorMode"
                v-model="layerVisibility.railway"
                @change="toggleLayer('railway')"
              >
              <span class="layer-name">🚂 Railways</span>
            </label>
            <label class="layer-toggle" :class="{ disabled: !isVectorMode }">
              <input
                type="checkbox"
                :disabled="!isVectorMode"
                v-model="layerVisibility.boundary"
                @change="toggleLayer('boundary')"
              >
              <span class="layer-name">🗺️ Boundaries</span>
            </label>
            <label class="layer-toggle" :class="{ disabled: !isVectorMode }">
              <input
                type="checkbox"
                :disabled="!isVectorMode"
                v-model="layerVisibility.labels"
                @change="toggleLayer('labels')"
              >
              <span class="layer-name">🔤 Labels</span>
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
              :class="{ active: activeTool === 'poi' }"
              @click="toggleTool('poi')"
              title="Add and edit custom Points of Interest"
            >
              🏷️ {{ activeTool === 'poi' ? 'Close POI Editor' : 'Edit POIs' }}
            </button>
            <div v-if="activeTool === 'poi'" class="tool-sub-panel">
              <div class="poi-file-row">
                <select v-model="selectedPoiFile" @change="onPoiFileChange" class="poi-select">
                  <option value="">— select file —</option>
                  <option v-for="f in poiFiles" :key="f.filename" :value="f.filename">{{ f.filename }}</option>
                </select>
                <button class="tool-btn-icon" :class="{ active: showNewPoiFile }" @click="showNewPoiFile = !showNewPoiFile" title="Create new POI file">+</button>
              </div>
              <div v-if="showNewPoiFile" class="poi-create-row">
                <input
                  v-model="newPoiFileName"
                  placeholder="name.geojson"
                  class="poi-input"
                  @keydown.enter="createPoiFile"
                  @keydown.escape="showNewPoiFile = false"
                />
                <button class="tool-btn-secondary" :disabled="!newPoiFileName.trim()" @click="createPoiFile">Create</button>
              </div>
              <template v-if="selectedPoiFile && poiData">
                <p class="tool-hint">
                  Click the map to place a POI. {{ poiData.features.length }} POI{{ poiData.features.length !== 1 ? 's' : '' }} loaded.
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
                    <span class="poi-list-name">{{ feat.properties?.name || '(unnamed)' }}</span>
                    <button class="poi-remove-btn" @click="removePoi(idx)" title="Remove">✕</button>
                  </div>
                </div>
                <div class="poi-save-row">
                  <button
                    class="tool-btn"
                    :class="{ saved: !poiDirty }"
                    :disabled="!poiDirty || poiSaving"
                    @click="savePoiFile"
                  >
                    {{ poiSaving ? 'Saving…' : poiDirty ? 'Save Changes' : 'Saved ✓' }}
                  </button>
                </div>
              </template>
            </div>
          </div>

        </div>
      </div>

      <p class="map-note" v-if="selectedMap">
        {{ renderMode === 'vector'
          ? 'Vector tile mode. Layers can be styled and toggled.'
          : renderMode === 'raster'
            ? 'Raster tile mode. Tiles are pre-rendered and styling controls are limited.'
            : 'Detecting tile mode.' }}
      </p>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, watch, onBeforeUnmount, computed, nextTick } from 'vue'
import { apiService } from '../services/api'
import { useLocationState } from '../services/location'
import maplibregl from 'maplibre-gl'
import { PMTiles, Protocol } from 'pmtiles'
import 'maplibre-gl/dist/maplibre-gl.css'
import { formatBytes, formatDistance, pathLength } from '../modules/maps/geo'
import {
  buildMapDataUrl as buildMapDataUrlFor,
  getMapFormat,
  mbtilesMetadataUrl,
  mbtilesTileUrl,
  isVectorTileFormat,
  parseMbtilesVectorLayers
} from '../modules/maps/mapFiles'
import {
  emptyGeoJSON,
  normalizePoiData,
  normalizePoiFilename,
  makePointFeature,
  buildMeasureGeoJSON
} from '../modules/maps/poi'
import {
  MAP_SOURCE_ID,
  MEASURE_SOURCE_ID,
  POI_SOURCE_ID,
  layerGroups,
  defaultLayerVisibility,
  buildBaseStyle,
  buildVectorLayers,
  buildMeasureLayers,
  buildPoiLayers,
  poiLayerIds
} from '../modules/maps/style'

const maps = ref([])
const mapsLoading = ref(false)
const mapsError = ref(null)
const mapError = ref(null)
const selectedMap = ref(null)
const showSelector = ref(true)
const showLayers = ref(true)
const showInfo = ref(false)
const showTools = ref(false)
const renderMode = ref('unknown')
const layerVisibility = ref(defaultLayerVisibility())

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
const selectedPoiFile = ref('')
const poiData = ref(null)
const poiDirty = ref(false)
const poiSaving = ref(false)
const showNewPoiFile = ref(false)
const newPoiFileName = ref('')
const pendingPoiCoords = ref(null)
const pendingPoiName = ref('')
const poiNameInput = ref(null)

let mapInstance = null
let pmtilesProtocol = null
let locationMarker = null

const locationState = useLocationState()

const isVectorMode = computed(() => renderMode.value === 'vector')
const renderModeLabel = computed(() => {
  if (renderMode.value === 'vector') return 'Vector tiles'
  if (renderMode.value === 'raster') return 'Raster tiles'
  return 'Unknown'
})

const buildMapDataUrl = (filename) => buildMapDataUrlFor(filename, window.location.origin)

const selectMap = (map) => {
  selectedMap.value = map
}

const getInitialCenter = () => {
  const location = locationState.location
  if (location) {
    return [location.longitude, location.latitude]
  }

  return [13.4, 59.5]
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

// ── Center on user location (issue #43) ────────────────────────────
const centerOnUserLocation = () => {
  if (!mapInstance) return
  if (!navigator.geolocation) {
    locationError.value = 'Geolocation is not supported by this browser'
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
      locationError.value = err.code === 1 ? 'Location access was denied' : 'Could not determine location'
    },
    { timeout: 10000 }
  )
}

// ── Measure tool (issue #41) ────────────────────────────────────────
const getMeasureGeoJSON = () => buildMeasureGeoJSON(measurePoints.value)

const addMeasureLayers = () => {
  if (!mapInstance || mapInstance.getSource(MEASURE_SOURCE_ID)) return
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

// ── POI tool (issue #42) ────────────────────────────────────────────
const addPoiLayersToMap = () => {
  if (!mapInstance) return
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
  if (!selectedPoiFile.value) {
    poiData.value = null
    removePoiLayersFromMap()
    if (mapInstance) {
      mapInstance.off('click', onPoiClick)
      mapInstance.getCanvas().style.cursor = ''
    }
    return
  }
  const requestedFile = selectedPoiFile.value
  try {
    const data = await apiService.readPoi(requestedFile)
    if (selectedPoiFile.value !== requestedFile) return
    poiData.value = normalizePoiData(data)
    poiDirty.value = false
    addPoiLayersToMap()
    if (mapInstance) {
      mapInstance.off('click', onPoiClick)
      mapInstance.on('click', onPoiClick)
      mapInstance.getCanvas().style.cursor = 'crosshair'
    }
  } catch (err) {
    if (selectedPoiFile.value !== requestedFile) return
    console.error('Error loading POI file:', err)
    poiData.value = emptyGeoJSON()
  }
}

const createPoiFile = async () => {
  const filename = normalizePoiFilename(newPoiFileName.value)
  if (!filename) return
  const newData = emptyGeoJSON()
  try {
    await apiService.savePoi(filename, newData)
    await loadPoiFiles()
    selectedPoiFile.value = filename
    poiData.value = newData
    poiDirty.value = false
    showNewPoiFile.value = false
    newPoiFileName.value = ''
    addPoiLayersToMap()
    if (mapInstance) {
      mapInstance.on('click', onPoiClick)
      mapInstance.getCanvas().style.cursor = 'crosshair'
    }
  } catch (err) {
    console.error('Error creating POI file:', err)
  }
}

const onPoiClick = (e) => {
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
  if (!selectedPoiFile.value || !poiData.value) return
  poiSaving.value = true
  try {
    await apiService.savePoi(selectedPoiFile.value, poiData.value)
    poiDirty.value = false
  } catch (err) {
    console.error('Error saving POI file:', err)
  } finally {
    poiSaving.value = false
  }
}

// ── Tool activation / deactivation ─────────────────────────────────
const activateTool = (tool) => {
  if (!mapInstance) return
  if (tool === 'measure') {
    mapInstance.getCanvas().style.cursor = 'crosshair'
    addMeasureLayers()
    mapInstance.on('click', onMeasureClick)
  } else if (tool === 'poi') {
    loadPoiFiles()
    if (selectedPoiFile.value && poiData.value) {
      mapInstance.off('click', onPoiClick)
      mapInstance.on('click', onPoiClick)
      mapInstance.getCanvas().style.cursor = 'crosshair'
    }
  }
}

const deactivateTool = (tool) => {
  if (!mapInstance) return
  if (tool === 'measure') {
    mapInstance.getCanvas().style.cursor = ''
    mapInstance.off('click', onMeasureClick)
  } else if (tool === 'poi') {
    if (pendingPoiCoords.value) cancelPoi()
    mapInstance.getCanvas().style.cursor = ''
    mapInstance.off('click', onPoiClick)
  }
}

const toggleTool = (tool) => {
  if (activeTool.value === tool) {
    deactivateTool(tool)
    activeTool.value = 'none'
  } else {
    deactivateTool(activeTool.value)
    activeTool.value = tool
    activateTool(tool)
  }
}

// Re-attach tool layers/handlers after a map re-initialization
const afterMapLoaded = () => {
  if (activeTool.value === 'measure') {
    addMeasureLayers()
    mapInstance.on('click', onMeasureClick)
    mapInstance.getCanvas().style.cursor = 'crosshair'
  } else if (activeTool.value === 'poi' && selectedPoiFile.value && poiData.value) {
    addPoiLayersToMap()
    mapInstance.on('click', onPoiClick)
    mapInstance.getCanvas().style.cursor = 'crosshair'
  }
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

const getMapDiagnostics = async (filename) => {
  const rawUrl = buildMapDataUrl(filename)
  const pmtilesArchive = new PMTiles(rawUrl)
  const header = await pmtilesArchive.getHeader()
  const metadata = await pmtilesArchive.getMetadata()
  const vectorLayers = Array.isArray(metadata?.vector_layers)
    ? metadata.vector_layers.map((layer) => layer.id)
    : []

  console.log('PMTiles diagnostics:', {
    filename,
    tileType: header.tileType,
    minZoom: header.minZoom,
    maxZoom: header.maxZoom,
    vectorLayers
  })

  return { header, vectorLayers }
}

const tryLoadMbtilesSource = async (filename) => {
  try {
    let meta = null
    try {
      const res = await fetch(mbtilesMetadataUrl(filename))
      if (res.ok) meta = await res.json()
    } catch (_) { /* use default */ }
    const tileFormat = meta?.format || 'png'

    const tileUrl = mbtilesTileUrl(filename)

    if (isVectorTileFormat(tileFormat)) {
      const vectorLayers = parseMbtilesVectorLayers(meta)
      mapInstance.addSource(MAP_SOURCE_ID, {
        type: 'vector',
        tiles: [tileUrl],
        minzoom: 0,
        maxzoom: 14
      })
      addVectorLayers(vectorLayers)
      renderMode.value = 'vector'
    } else {
      mapInstance.addSource(MAP_SOURCE_ID, {
        type: 'raster',
        tiles: [tileUrl],
        tileSize: 256,
        minzoom: 0,
        maxzoom: 19
      })
      mapInstance.addLayer({
        id: 'raster-layer',
        type: 'raster',
        source: MAP_SOURCE_ID,
        paint: { 'raster-opacity': 1 }
      })
      renderMode.value = 'raster'
    }
    return true
  } catch (error) {
    console.warn(`MBTiles load failed: ${error.message}`)
    return false
  }
}

const tryLoadVectorSource = (pmtilesUrl, header, availableLayers) => {
  try {
    mapInstance.addSource(MAP_SOURCE_ID, {
      type: 'vector',
      url: pmtilesUrl,
      minzoom: header?.minZoom ?? 0,
      maxzoom: header?.maxZoom ?? 28
    })
    addVectorLayers(availableLayers || [])
    renderMode.value = 'vector'
    return true
  } catch (error) {
    console.warn(`Vector load failed: ${error.message}`)
    return false
  }
}

const tryLoadRasterSource = (pmtilesUrl, header) => {
  try {
    mapInstance.addSource(MAP_SOURCE_ID, {
      type: 'raster',
      url: pmtilesUrl,
      tileSize: 256,
      minzoom: header?.minZoom ?? 0,
      maxzoom: header?.maxZoom ?? 22
    })

    mapInstance.addLayer({
      id: 'raster-layer',
      type: 'raster',
      source: MAP_SOURCE_ID,
      paint: {
        'raster-opacity': 1
      }
    })

    renderMode.value = 'raster'
    return true
  } catch (error) {
    console.warn(`Raster load failed: ${error.message}`)
    return false
  }
}

const addVectorLayers = (availableLayers) => {
  const { layers, skipped, mapping } = buildVectorLayers(availableLayers, MAP_SOURCE_ID)
  console.log('Vector schema mapping:', { availableLayers, ...mapping })
  for (const def of skipped) {
    console.warn(`Skipping layer '${def.id}', missing source-layer '${def.layer}'`)
  }

  let layersAdded = 0
  for (const layer of layers) {
    try {
      mapInstance.addLayer(layer)
      layersAdded++
    } catch (error) {
      console.warn(`Layer add failed for '${layer.id}': ${error.message}`)
    }
  }
  console.log(`Vector layers added: ${layersAdded}`)

  for (const groupName of Object.keys(layerGroups)) {
    setGroupVisibility(groupName, layerVisibility.value[groupName])
  }
}

const initializeMap = async () => {
  mapError.value = null
  try {
    if (mapInstance) {
      mapInstance.remove()
      mapInstance = null
    }

    const mapContainer = document.getElementById('map')
    if (!mapContainer || !selectedMap.value) {
      return
    }

    mapInstance = new maplibregl.Map({
      container: mapContainer,
      style: buildBaseStyle(),
      center: getInitialCenter(),
      zoom: 6,
      pitch: 0,
      bearing: 0
    })

    mapInstance.addControl(new maplibregl.NavigationControl(), 'top-right')
    mapInstance.addControl(new maplibregl.ScaleControl(), 'bottom-right')

    mapInstance.on('load', async () => {
      try {
        const filename = selectedMap.value.filename
        const format = getMapFormat(filename)

        let loaded = false

        if (format === 'mbtiles') {
          loaded = await tryLoadMbtilesSource(filename)
        } else {
          // PMTiles path
          const pmtilesUrl = `pmtiles://${buildMapDataUrl(filename)}`

          let header = null
          let vectorLayers = []
          let tileType = null

          try {
            const diagnostics = await getMapDiagnostics(filename)
            header = diagnostics.header
            vectorLayers = diagnostics.vectorLayers
            tileType = diagnostics.header?.tileType
          } catch (diagError) {
            console.warn(`PMTiles diagnostics failed, using fallback loading: ${diagError.message}`)
          }

          if (tileType === 1 || tileType === 6) {
            loaded = tryLoadVectorSource(pmtilesUrl, header, vectorLayers)
            if (!loaded) loaded = tryLoadRasterSource(pmtilesUrl, header)
          } else if (tileType >= 2 && tileType <= 5) {
            loaded = tryLoadRasterSource(pmtilesUrl, header)
            if (!loaded) loaded = tryLoadVectorSource(pmtilesUrl, header, vectorLayers)
          } else {
            loaded = tryLoadVectorSource(pmtilesUrl, header, vectorLayers)
            if (!loaded) loaded = tryLoadRasterSource(pmtilesUrl, header)
          }
        }

        if (!loaded) {
          renderMode.value = 'unknown'
          mapError.value = 'Unable to render the selected map.'
          console.warn(`Unable to load map source for ${filename}`)
        }

        updateLocationMarker(locationState.location)
        afterMapLoaded()
      } catch (error) {
        renderMode.value = 'unknown'
        mapError.value = 'Failed to load map data.'
        console.error('Map load error:', error.message)
      }
    })

    mapInstance.on('error', (event) => {
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
  if (mapInstance) {
    mapInstance.off('click', onMeasureClick)
    mapInstance.off('click', onPoiClick)
    mapInstance.remove()
    mapInstance = null
  }
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

onMounted(async () => {
  if (!pmtilesProtocol) {
    pmtilesProtocol = new Protocol()
    maplibregl.addProtocol('pmtiles', pmtilesProtocol.tile)
  }

  await loadMaps()
})

onBeforeUnmount(() => {
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
  }
}
</style>