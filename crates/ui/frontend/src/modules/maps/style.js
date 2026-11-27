// Pure MapLibre style/layer builders for the Maps page.
// Nothing in here touches a live map instance, so it can be unit-tested.

export const MAP_SOURCE_ID = 'pmtiles-source'

/** Layer-toggle groups → MapLibre layer ids they control. */
export const layerGroups = {
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

export const defaultLayerVisibility = () => ({
  water: true,
  landuse: true,
  park: true,
  building: true,
  road: true,
  railway: true,
  boundary: true,
  labels: true
})

/** Minimal empty style used when a map is first created. */
export const buildBaseStyle = () => ({
  version: 8,
  sources: {},
  layers: [{
    id: 'background',
    type: 'background',
    paint: { 'background-color': '#f2efe9' }
  }]
})

/**
 * Map the source-layer names a tileset advertises (Protomaps, Shortbread,
 * OpenMapTiles, …) onto the logical layers the default style needs.
 */
export const resolveSourceLayers = (availableLayers) => {
  const availableList = Array.isArray(availableLayers) ? availableLayers : []
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

  return {
    earthSource: pickLayer(['land', 'earth'], 'land', [/^land$/i, /^earth$/i]),
    oceanSource: pickLayer(['ocean'], 'ocean', [/^ocean$/i]),
    landcoverSource: pickLayer(['landcover', 'landuse', 'land'], 'land', [/landcover/i, /landuse/i, /^land$/i]),
    landuseSource: pickLayer(['landuse', 'landcover', 'land'], 'land', [/landuse/i, /landcover/i, /^land$/i]),
    waterSource: pickLayer(['water_polygons', 'water'], 'water_polygons', [/water_polygon/i, /^water$/i, /hydro/i]),
    waterLineSource: pickLayer(['water_lines', 'waterway', 'physical_line', 'water'], 'water_lines', [/waterway/i, /river/i, /stream/i, /canal/i, /physical_line/i, /water_line/i]),
    buildingSource: pickLayer(['buildings', 'building'], 'buildings', [/building/i]),
    roadsSource: pickLayer(['streets', 'roads', 'transportation', 'road'], 'streets', [/transport/i, /road/i, /street/i, /highway/i]),
    transitSource: pickLayer(['transit', 'transportation', 'roads', 'railway'], 'transit', [/rail/i, /transit/i, /transport/i]),
    boundarySource: pickLayer(['boundaries', 'boundary', 'admin'], 'boundaries', [/boundar/i, /admin/i]),
    placesSource: pickLayer(['places', 'place'], 'places', [/places?/i, /settlement/i]),
    poisSource: pickLayer(['pois', 'poi'], 'pois', [/pois?/i, /point/i]),
    sitesSource: pickLayer(['sites'], 'sites', [/^sites$/i])
  }
}

/**
 * Build the default vector style layer definitions (without source binding).
 * Each entry has `{ id, layer, type, minzoom?, filter?, paint?, layout? }`
 * where `layer` is the resolved source-layer name.
 */
export const buildVectorLayerDefs = (availableLayers) => {
  const {
    earthSource,
    oceanSource,
    landcoverSource,
    landuseSource,
    waterSource,
    waterLineSource,
    buildingSource,
    roadsSource,
    transitSource,
    boundarySource,
    placesSource,
    poisSource,
    sitesSource
  } = resolveSourceLayers(availableLayers)

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

  return [
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
      id: 'ocean-layer',
      layer: oceanSource,
      type: 'fill',
      filter: ['==', '$type', 'Polygon'],
      paint: {
        'fill-color': '#b3d9ff',
        'fill-opacity': 0.9
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
      id: 'sites-layer',
      layer: sitesSource,
      type: 'fill',
      minzoom: 14,
      filter: ['==', '$type', 'Polygon'],
      paint: {
        'fill-color': '#e2ddd2',
        'fill-opacity': 0.7
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
        'line-dasharray': ['match', roadClassExpr,
          'track', ['literal', [3, 2]],
          'path', ['literal', [1, 3]],
          ['literal', [1.5, 1.6]]
        ],
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
}

/**
 * Build ready-to-add MapLibre layer specs for a vector tileset.
 * Layers whose source-layer is not advertised by the tileset are reported in
 * `skipped` (when the tileset advertises nothing, every layer is attempted).
 */
export const buildVectorLayers = (availableLayers, sourceId = MAP_SOURCE_ID) => {
  const availableList = Array.isArray(availableLayers) ? availableLayers : []
  const available = new Set(availableList)
  const layers = []
  const skipped = []

  for (const layerDef of buildVectorLayerDefs(availableList)) {
    if (availableList.length > 0 && !available.has(layerDef.layer)) {
      skipped.push(layerDef)
      continue
    }

    const layer = {
      id: layerDef.id,
      type: layerDef.type,
      source: sourceId,
      'source-layer': layerDef.layer,
      layout: { visibility: 'visible' }
    }

    if (layerDef.minzoom !== undefined) layer.minzoom = layerDef.minzoom
    if (layerDef.filter) layer.filter = layerDef.filter
    if (layerDef.layout) layer.layout = { ...layer.layout, ...layerDef.layout }
    if (layerDef.paint) layer.paint = layerDef.paint

    layers.push(layer)
  }

  return { layers, skipped, mapping: resolveSourceLayers(availableList) }
}

// ── Overlay layers (measure tool, POI files) ───────────────────────

export const MEASURE_SOURCE_ID = 'measure-source'

export const buildMeasureLayers = (sourceId = MEASURE_SOURCE_ID) => [
  {
    id: 'measure-line-layer',
    type: 'line',
    source: sourceId,
    filter: ['==', '$type', 'LineString'],
    paint: { 'line-color': '#f7c948', 'line-width': 2, 'line-dasharray': [3, 2] }
  },
  {
    id: 'measure-points-layer',
    type: 'circle',
    source: sourceId,
    filter: ['==', '$type', 'Point'],
    paint: { 'circle-radius': 5, 'circle-color': '#f7c948', 'circle-stroke-width': 2, 'circle-stroke-color': '#fff' }
  }
]

export const POI_SOURCE_ID = 'poi-source'

export const buildPoiLayers = (sourceId = POI_SOURCE_ID) => [
  {
    id: 'poi-fill-layer',
    type: 'fill',
    source: sourceId,
    filter: ['==', '$type', 'Polygon'],
    paint: { 'fill-color': '#ff6b35', 'fill-opacity': 0.18, 'fill-outline-color': '#ff6b35' }
  },
  {
    id: 'poi-line-casing-layer',
    type: 'line',
    source: sourceId,
    filter: ['==', '$type', 'LineString'],
    layout: { 'line-cap': 'round', 'line-join': 'round' },
    paint: { 'line-color': '#ffffff', 'line-width': 5, 'line-opacity': 0.85 }
  },
  {
    id: 'poi-line-layer',
    type: 'line',
    source: sourceId,
    filter: ['==', '$type', 'LineString'],
    layout: { 'line-cap': 'round', 'line-join': 'round' },
    paint: { 'line-color': '#ff6b35', 'line-width': 3 }
  },
  {
    id: 'poi-circle-layer',
    type: 'circle',
    source: sourceId,
    filter: ['==', '$type', 'Point'],
    paint: { 'circle-radius': 7, 'circle-color': '#ff6b35', 'circle-stroke-width': 2, 'circle-stroke-color': '#fff' }
  },
  {
    id: 'poi-label-layer',
    type: 'symbol',
    source: sourceId,
    filter: ['==', '$type', 'Point'],
    layout: {
      'text-field': ['get', 'name'],
      'text-offset': [0, 1.3],
      'text-anchor': 'top',
      'text-size': 12,
      'text-allow-overlap': false
    },
    paint: { 'text-color': '#fff', 'text-halo-color': '#000', 'text-halo-width': 1.2 }
  }
]

/** Layers whose features open an info popup when clicked. */
export const POI_INTERACTIVE_LAYER_IDS = ['poi-circle-layer', 'poi-line-layer', 'poi-fill-layer']

/** Layer ids of the POI overlay, in removal order (top-most first). */
export const poiLayerIds = (sourceId = POI_SOURCE_ID) => buildPoiLayers(sourceId).map((l) => l.id).reverse()

export const TRACK_SOURCE_ID = 'track-source'

/** Live/unsaved GPS track overlay. */
export const buildTrackLayers = (sourceId = TRACK_SOURCE_ID) => [
  {
    id: 'track-line-layer',
    type: 'line',
    source: sourceId,
    layout: { 'line-cap': 'round', 'line-join': 'round' },
    paint: { 'line-color': '#e63946', 'line-width': 4, 'line-opacity': 0.9 }
  }
]
