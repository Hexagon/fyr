<template>
  <div class="maps-page">
    <div class="maps-container">
      <div id="map" class="map-canvas"></div>

      <div class="overlay overlay-selector" :class="{ collapsed: !showSelector }">
        <button class="overlay-toggle" @click="showSelector = !showSelector" :title="showSelector ? 'Hide maps' : 'Show maps'">
          {{ showSelector ? '🗺️' : '📂' }}
        </button>
        <div v-if="showSelector" class="overlay-content">
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
        <button class="overlay-toggle" @click="showLayers = !showLayers" :title="showLayers ? 'Hide layers' : 'Show layers'">
          {{ showLayers ? '🎨' : '🧩' }}
        </button>
        <div v-if="showLayers" class="overlay-content">
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
        <button class="overlay-toggle" @click="showInfo = !showInfo" :title="showInfo ? 'Hide info' : 'Show info'">
          {{ showInfo ? 'ℹ️' : '📍' }}
        </button>
        <div v-if="showInfo" class="overlay-content">
          <h3>{{ selectedMap.filename }}</h3>
          <p class="overlay-meta">📍 {{ selectedMap.path }}</p>
          <p class="overlay-meta">📊 {{ formatBytes(selectedMap.size) }}</p>
          <p class="overlay-meta">⏰ {{ new Date(selectedMap.modified).toLocaleDateString() }}</p>
          <p class="overlay-meta">Mode: {{ renderModeLabel }}</p>
        </div>
      </div>

      <div v-if="!selectedMap" class="empty-view">
        <p>{{ mapsLoading ? 'Loading maps...' : '👈 Select a map to view' }}</p>
      </div>

      <div v-if="mapError" class="map-error-banner">{{ mapError }}</div>

      <p class="map-note" v-if="selectedMap">
        {{ renderMode === 'vector'
          ? 'Vector PMTiles mode. Layers can be styled and toggled.'
          : renderMode === 'raster'
            ? 'Raster PMTiles mode. Tiles are pre-rendered and styling controls are limited.'
            : 'Detecting tile mode.' }}
      </p>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, watch, onBeforeUnmount, computed } from 'vue'
import { apiService } from '../services/api'
import { useLocationState } from '../services/location'
import maplibregl from 'maplibre-gl'
import { PMTiles, Protocol } from 'pmtiles'
import 'maplibre-gl/dist/maplibre-gl.css'

const maps = ref([])
const mapsLoading = ref(false)
const mapsError = ref(null)
const mapError = ref(null)
const selectedMap = ref(null)
const showSelector = ref(true)
const showLayers = ref(true)
const showInfo = ref(false)
const renderMode = ref('unknown')
const layerVisibility = ref({
  water: true,
  landuse: true,
  park: true,
  building: true,
  road: true,
  railway: true,
  boundary: true,
  labels: true
})

const layerGroups = {
  water: ['water-layer', 'water-line-layer'],
  landuse: ['landuse-layer'],
  park: ['park-layer'],
  building: ['building-layer'],
  road: [
    'road-major-casing-layer',
    'road-major-layer',
    'road-minor-casing-layer',
    'road-minor-layer',
    'road-any-layer',
    'road-track-layer',
    'road-walkway-layer',
    'road-name-layer'
  ],
  railway: ['railway-layer'],
  boundary: ['boundary-layer'],
  labels: ['place-name-layer', 'poi-name-layer', 'road-name-layer']
}

let mapInstance = null
let pmtilesProtocol = null
let locationMarker = null

const locationState = useLocationState()

const isVectorMode = computed(() => renderMode.value === 'vector')
const renderModeLabel = computed(() => {
  if (renderMode.value === 'vector') return 'Vector PMTiles'
  if (renderMode.value === 'raster') return 'Raster PMTiles'
  return 'Unknown'
})

const formatBytes = (bytes) => {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i]
}

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
  const rawUrl = `http://127.0.0.1:8080/data/maps/${filename}`
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

const tryLoadVectorSource = (pmtilesUrl, header, availableLayers) => {
  try {
    mapInstance.addSource('pmtiles-source', {
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
    mapInstance.addSource('pmtiles-source', {
      type: 'raster',
      url: pmtilesUrl,
      tileSize: 256,
      minzoom: header?.minZoom ?? 0,
      maxzoom: header?.maxZoom ?? 22
    })

    mapInstance.addLayer({
      id: 'raster-layer',
      type: 'raster',
      source: 'pmtiles-source',
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
  const availableList = Array.isArray(availableLayers) ? availableLayers : []
  const available = new Set(availableList)
  const lowerMap = new Map(availableList.map((name) => [String(name).toLowerCase(), name]))

  const pickLayer = (candidates, fallback, patternHints = []) => {
    for (const candidate of candidates) {
      const exact = lowerMap.get(String(candidate).toLowerCase())
      if (exact) return exact
    }

    for (const hint of patternHints) {
      const found = availableList.find((layerName) => hint.test(String(layerName)))
      if (found) return found
    }

    if (availableList.length === 0) return fallback
    return lowerMap.get(String(fallback).toLowerCase()) || fallback
  }

  const earthSource = pickLayer(['earth'], 'earth')
  const landcoverSource = pickLayer(['landcover', 'landuse'], 'landcover', [/landcover/i, /landuse/i])
  const landuseSource = pickLayer(['landuse', 'landcover'], 'landuse', [/landuse/i, /landcover/i])
  const waterSource = pickLayer(['water'], 'water', [/^water$/i, /hydro/i, /water/i])
  const waterLineSource = pickLayer(['waterway', 'physical_line', 'water'], 'waterway', [/waterway/i, /river/i, /stream/i, /canal/i, /physical_line/i])
  const buildingSource = pickLayer(['buildings', 'building'], 'buildings', [/building/i])
  const roadsSource = pickLayer(['roads', 'transportation', 'road'], 'roads', [/transport/i, /road/i, /street/i, /highway/i])
  const transitSource = pickLayer(['transit', 'transportation', 'roads', 'railway'], 'transit', [/rail/i, /transit/i, /transport/i])
  const boundarySource = pickLayer(['boundaries', 'boundary', 'admin'], 'boundaries', [/boundar/i, /admin/i])
  const placesSource = pickLayer(['places', 'place'], 'places', [/places?/i, /settlement/i])
  const poisSource = pickLayer(['pois', 'poi'], 'pois', [/pois?/i, /point/i])

  const roadClassExpr = ['coalesce', ['get', 'class'], ['get', 'kind'], ['get', 'type'], ['get', 'highway']]
  const roadDetailExpr = ['coalesce', ['get', 'kind_detail'], ['get', 'detail'], ['get', 'subclass']]

  const majorRoadValues = ['motorway', 'trunk', 'primary', 'secondary', 'motorway_link', 'trunk_link', 'primary_link', 'secondary_link']
  const minorRoadValues = ['tertiary', 'minor', 'street', 'residential', 'living_street', 'service', 'unclassified']
  const trackRoadValues = ['track', 'path', 'cycleway', 'bridleway']
  const walkwayRoadValues = ['footway', 'pedestrian', 'steps', 'sidewalk', 'corridor']
  const railValues = ['rail', 'railway', 'tram', 'light_rail', 'subway']

  const parkFilter = ['any',
    ['in', 'class', 'park', 'nature_reserve', 'garden'],
    ['in', 'kind', 'park', 'nature_reserve', 'garden'],
    ['in', 'type', 'park', 'nature_reserve', 'garden']
  ]

  const waterLineFilter = ['any',
    ['in', 'class', 'river', 'stream', 'canal', 'drain'],
    ['in', 'kind', 'river', 'stream', 'canal', 'drain'],
    ['in', 'type', 'river', 'stream', 'canal', 'drain']
  ]

  const majorRoadFilter = ['any',
    ['in', 'class', ...majorRoadValues],
    ['in', 'kind', ...majorRoadValues],
    ['in', 'type', ...majorRoadValues],
    ['in', 'highway', ...majorRoadValues],
    ['in', 'kind_detail', ...majorRoadValues],
    ['in', 'detail', ...majorRoadValues],
    ['in', 'subclass', ...majorRoadValues]
  ]

  const minorRoadFilter = ['any',
    ['in', 'class', ...minorRoadValues],
    ['in', 'kind', ...minorRoadValues],
    ['in', 'type', ...minorRoadValues],
    ['in', 'highway', ...minorRoadValues],
    ['in', 'kind_detail', ...minorRoadValues],
    ['in', 'detail', ...minorRoadValues],
    ['in', 'subclass', ...minorRoadValues]
  ]

  const trackRoadFilter = ['any',
    ['in', 'class', ...trackRoadValues],
    ['in', 'kind', ...trackRoadValues],
    ['in', 'type', ...trackRoadValues],
    ['in', 'highway', ...trackRoadValues],
    ['in', 'kind_detail', ...trackRoadValues],
    ['in', 'detail', ...trackRoadValues],
    ['in', 'subclass', ...trackRoadValues]
  ]

  const walkwayRoadFilter = ['any',
    ['in', 'class', ...walkwayRoadValues],
    ['in', 'kind', ...walkwayRoadValues],
    ['in', 'type', ...walkwayRoadValues],
    ['in', 'highway', ...walkwayRoadValues],
    ['in', 'kind_detail', ...walkwayRoadValues],
    ['in', 'detail', ...walkwayRoadValues],
    ['in', 'subclass', ...walkwayRoadValues]
  ]

  const railFilter = ['any',
    ['in', 'class', ...railValues],
    ['in', 'kind', ...railValues],
    ['in', 'type', ...railValues],
    ['in', 'highway', ...railValues],
    ['in', 'kind_detail', ...railValues],
    ['in', 'detail', ...railValues],
    ['in', 'subclass', ...railValues]
  ]

  console.log('Vector schema mapping:', {
    availableLayers: availableList,
    roadsSource,
    transitSource,
    landuseSource,
    waterSource,
    waterLineSource,
    boundarySource,
    placesSource,
    poisSource
  })

  const vectorLayers = [
    {
      id: 'earth-layer',
      layer: earthSource,
      type: 'fill',
      filter: ['==', '$type', 'Polygon'],
      paint: {
        'fill-color': '#f7f4ee',
        'fill-opacity': 1
      }
    },
    {
      id: 'landcover-layer',
      layer: landcoverSource,
      type: 'fill',
      filter: ['==', '$type', 'Polygon'],
      paint: {
        'fill-color': '#dfe8c8',
        'fill-opacity': 0.55
      }
    },
    {
      id: 'landuse-layer',
      layer: landuseSource,
      type: 'fill',
      filter: ['all', ['==', '$type', 'Polygon']],
      paint: {
        'fill-color': '#d8e6b4',
        'fill-opacity': 0.65
      }
    },
    {
      id: 'park-layer',
      layer: landuseSource,
      type: 'fill',
      minzoom: 7,
      filter: ['all',
        ['==', '$type', 'Polygon'],
        parkFilter
      ],
      paint: {
        'fill-color': '#c6ddb0',
        'fill-opacity': 0.75
      }
    },
    {
      id: 'water-layer',
      layer: waterSource,
      type: 'fill',
      filter: ['all',
        ['==', '$type', 'Polygon']
      ],
      paint: {
        'fill-color': '#99ccff',
        'fill-opacity': 0.9
      }
    },
    {
      id: 'water-line-layer',
      layer: waterLineSource,
      type: 'line',
      minzoom: 6,
      filter: ['all',
        ['==', '$type', 'LineString'],
        waterLineFilter
      ],
      paint: {
        'line-color': '#5fa8e8',
        'line-width': ['interpolate', ['linear'], ['zoom'], 6, 0.7, 10, 1.4, 14, 2.6],
        'line-opacity': 0.9
      }
    },
    {
      id: 'building-layer',
      layer: buildingSource,
      type: 'fill',
      minzoom: 11,
      filter: ['==', '$type', 'Polygon'],
      paint: {
        'fill-color': '#e4d8d8',
        'fill-opacity': 0.78
      }
    },
    {
      id: 'road-major-casing-layer',
      layer: roadsSource,
      type: 'line',
      minzoom: 4,
      filter: ['all',
        ['==', '$type', 'LineString'],
        majorRoadFilter
      ],
      paint: {
        'line-color': '#efe9dc',
        'line-width': ['interpolate', ['linear'], ['zoom'], 4, 1.6, 8, 3.4, 12, 8.4, 15, 12.5],
        'line-opacity': 1
      },
      layout: {
        'line-cap': 'round',
        'line-join': 'round'
      }
    },
    {
      id: 'road-major-layer',
      layer: roadsSource,
      type: 'line',
      minzoom: 4,
      filter: ['all',
        ['==', '$type', 'LineString'],
        majorRoadFilter
      ],
      paint: {
        'line-color': ['match', roadClassExpr,
          'motorway', '#d88c5a',
          'trunk', '#d99c67',
          'primary', '#ddb178',
          '#e4c89a'
        ],
        'line-width': ['interpolate', ['linear'], ['zoom'], 4, 0.9, 8, 2.0, 12, 5.8, 15, 9],
        'line-opacity': 0.98
      },
      layout: {
        'line-cap': 'round',
        'line-join': 'round'
      }
    },
    {
      id: 'road-minor-casing-layer',
      layer: roadsSource,
      type: 'line',
      minzoom: 8,
      filter: ['all',
        ['==', '$type', 'LineString'],
        minorRoadFilter
      ],
      paint: {
        'line-color': '#f2eee6',
        'line-width': ['interpolate', ['linear'], ['zoom'], 8, 1.1, 12, 3.2, 15, 5.2],
        'line-opacity': 0.95
      },
      layout: {
        'line-cap': 'round',
        'line-join': 'round'
      }
    },
    {
      id: 'road-minor-layer',
      layer: roadsSource,
      type: 'line',
      minzoom: 8,
      filter: ['all',
        ['==', '$type', 'LineString'],
        minorRoadFilter
      ],
      paint: {
        'line-color': ['match', roadClassExpr,
          'tertiary', '#d9bf97',
          'minor', '#d2b792',
          'street', '#cdb28f',
          'residential', '#c7ad88',
          '#bea47f'
        ],
        'line-width': ['interpolate', ['linear'], ['zoom'], 8, 0.5, 12, 1.6, 15, 3],
        'line-opacity': 0.95
      },
      layout: {
        'line-cap': 'round',
        'line-join': 'round'
      }
    },
    {
      id: 'road-any-layer',
      layer: roadsSource,
      type: 'line',
      minzoom: 9,
      filter: ['all',
        ['==', '$type', 'LineString']
      ],
      paint: {
        'line-color': '#ad9a79',
        'line-width': ['interpolate', ['linear'], ['zoom'], 9, 0.5, 12, 1.2, 15, 2.4],
        'line-opacity': 0.72
      },
      layout: {
        'line-cap': 'round',
        'line-join': 'round'
      }
    },
    {
      id: 'road-track-layer',
      layer: roadsSource,
      type: 'line',
      minzoom: 11,
      filter: ['all',
        ['==', '$type', 'LineString'],
        trackRoadFilter
      ],
      paint: {
        'line-color': ['match', roadClassExpr,
          'track', '#9b8b62',
          'path', '#7fa06d',
          'cycleway', '#6f9f78',
          '#8ea474'
        ],
        'line-width': ['interpolate', ['linear'], ['zoom'], 11, 0.45, 14, 1.2, 16, 1.8],
        'line-dasharray': [1.5, 1.6],
        'line-opacity': 0.88
      },
      layout: {
        'line-cap': 'round',
        'line-join': 'round'
      }
    },
    {
      id: 'road-walkway-layer',
      layer: roadsSource,
      type: 'line',
      minzoom: 12,
      filter: ['all',
        ['==', '$type', 'LineString'],
        walkwayRoadFilter
      ],
      paint: {
        'line-color': '#b3a892',
        'line-width': ['interpolate', ['linear'], ['zoom'], 12, 0.35, 15, 0.95, 17, 1.35],
        'line-dasharray': [0.8, 1.2],
        'line-opacity': 0.82
      },
      layout: {
        'line-cap': 'round',
        'line-join': 'round'
      }
    },
    {
      id: 'railway-layer',
      layer: transitSource,
      type: 'line',
      minzoom: 5,
      filter: ['all',
        ['==', '$type', 'LineString'],
        railFilter
      ],
      paint: {
        'line-color': '#8e8e8e',
        'line-width': ['interpolate', ['linear'], ['zoom'], 5, 0.6, 10, 1.4, 15, 2.8],
        'line-dasharray': [2, 2]
      },
      layout: {
        'line-cap': 'round',
        'line-join': 'round'
      }
    },
    {
      id: 'boundary-layer',
      layer: boundarySource,
      type: 'line',
      filter: ['==', '$type', 'LineString'],
      paint: {
        'line-color': '#b8b8b8',
        'line-width': ['interpolate', ['linear'], ['zoom'], 3, 0.4, 8, 0.8, 12, 1.3],
        'line-dasharray': [3, 2]
      },
      layout: {
        'line-cap': 'round',
        'line-join': 'round'
      }
    },
    {
      id: 'road-name-layer',
      layer: roadsSource,
      type: 'symbol',
      minzoom: 11,
      filter: ['all',
        ['==', '$type', 'LineString'],
        ['none', railFilter, trackRoadFilter, walkwayRoadFilter]
      ],
      layout: {
        'text-field': ['coalesce', ['get', 'name:en'], ['get', 'name'], ['get', 'ref']],
        'text-font': ['Open Sans Semibold', 'Arial Unicode MS Regular'],
        'text-size': ['interpolate', ['linear'], ['zoom'], 11, 11, 15, 14],
        'symbol-placement': 'line',
        'text-rotation-alignment': 'map',
        'text-allow-overlap': false
      },
      paint: {
        'text-color': '#303030',
        'text-halo-color': '#ffffff',
        'text-halo-width': 1.6
      }
    },
    {
      id: 'place-name-layer',
      layer: placesSource,
      type: 'symbol',
      minzoom: 5,
      layout: {
        'text-field': ['coalesce', ['get', 'name:en'], ['get', 'name']],
        'text-font': ['Open Sans Semibold', 'Arial Unicode MS Bold'],
        'text-size': ['interpolate', ['linear'], ['zoom'], 5, 10, 10, 14, 14, 18],
        'text-anchor': 'center',
        'text-allow-overlap': false,
        'text-ignore-placement': false
      },
      paint: {
        'text-color': '#222222',
        'text-halo-color': '#ffffff',
        'text-halo-width': 1.6
      }
    },
    {
      id: 'poi-name-layer',
      layer: poisSource,
      type: 'symbol',
      minzoom: 12,
      layout: {
        'text-field': ['coalesce', ['get', 'name:en'], ['get', 'name']],
        'text-font': ['Open Sans Regular', 'Arial Unicode MS Regular'],
        'text-size': ['interpolate', ['linear'], ['zoom'], 12, 10, 16, 13],
        'text-anchor': 'top',
        'text-offset': [0, 0.8],
        'text-allow-overlap': false,
        'icon-allow-overlap': false
      },
      paint: {
        'text-color': '#2a2a2a',
        'text-halo-color': '#ffffff',
        'text-halo-width': 1.4
      }
    }
  ]

  let layersAdded = 0

  for (const layerDef of vectorLayers) {
    if (availableLayers.length > 0 && !available.has(layerDef.layer)) {
      console.warn(`Skipping layer '${layerDef.id}', missing source-layer '${layerDef.layer}'`)
      continue
    }

    try {
      const layer = {
        id: layerDef.id,
        type: layerDef.type,
        source: 'pmtiles-source',
        'source-layer': layerDef.layer,
        layout: { visibility: 'visible' }
      }

      if (layerDef.minzoom !== undefined) layer.minzoom = layerDef.minzoom
      if (layerDef.filter) layer.filter = layerDef.filter
      if (layerDef.layout) layer.layout = { ...layer.layout, ...layerDef.layout }
      if (layerDef.paint) layer.paint = layerDef.paint

      mapInstance.addLayer(layer)
      layersAdded++
    } catch (error) {
      console.warn(`Layer add failed for '${layerDef.id}': ${error.message}`)
    }
  }

  console.log(`Vector layers added: ${layersAdded}`)

  setGroupVisibility('water', layerVisibility.value.water)
  setGroupVisibility('landuse', layerVisibility.value.landuse)
  setGroupVisibility('park', layerVisibility.value.park)
  setGroupVisibility('building', layerVisibility.value.building)
  setGroupVisibility('road', layerVisibility.value.road)
  setGroupVisibility('railway', layerVisibility.value.railway)
  setGroupVisibility('boundary', layerVisibility.value.boundary)
  setGroupVisibility('labels', layerVisibility.value.labels)
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
      style: {
        version: 8,
        sources: {},
        layers: [{
          id: 'background',
          type: 'background',
          paint: { 'background-color': '#f2efe9' }
        }]
      },
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
        const pmtilesUrl = `pmtiles://http://127.0.0.1:8080/data/maps/${filename}`

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

        let loaded = false

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

        if (!loaded) {
          renderMode.value = 'unknown'
          mapError.value = 'Unable to render the selected PMTiles archive.'
          console.warn(`Unable to load PMTiles source as either vector or raster for ${filename}`)
        }

        updateLocationMarker(locationState.location)
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
    mapInstance.remove()
    mapInstance = null
  }
}

watch(selectedMap, (newMap) => {
  if (newMap) {
    setTimeout(initializeMap, 120)
  } else {
    renderMode.value = 'unknown'
    cleanupMap()
  }
})

watch(() => locationState.location, (location) => {
  if (!mapInstance) return

  if (location) {
    mapInstance.setCenter([location.longitude, location.latitude])
    updateLocationMarker(location)
  } else {
    updateLocationMarker(null)
  }
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
  top: 0.75rem;
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

  .overlay-layers {
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
  .overlay-info,
  .overlay-layers {
    width: auto;
    left: 0.5rem;
    right: 0.5rem;
  }

  .overlay-selector {
    top: 0.5rem;
  }

  .overlay-layers {
    top: 3.4rem;
  }

  .overlay-info {
    bottom: 2.2rem;
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