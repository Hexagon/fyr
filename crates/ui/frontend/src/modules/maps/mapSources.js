// Attach a PMTiles or MBTiles archive to a MapLibre map as the base source.
// Works on any object with the MapLibre addSource/addLayer API, so it can be
// unit-tested with a fake map.

import { PMTiles } from 'pmtiles'
import {
  buildMapDataUrl,
  getMapFormat,
  mbtilesMetadataUrl,
  mbtilesTileUrl,
  isVectorTileFormat,
  parseMbtilesVectorLayers
} from './mapFiles.js'
import { MAP_SOURCE_ID, buildVectorLayers } from './style.js'

const addVectorLayers = (map, availableLayers) => {
  const { layers, skipped, mapping } = buildVectorLayers(availableLayers, MAP_SOURCE_ID)
  console.log('Vector schema mapping:', { availableLayers, ...mapping })
  for (const def of skipped) {
    console.warn(`Skipping layer '${def.id}', missing source-layer '${def.layer}'`)
  }

  let layersAdded = 0
  for (const layer of layers) {
    try {
      map.addLayer(layer)
      layersAdded++
    } catch (error) {
      console.warn(`Layer add failed for '${layer.id}': ${error.message}`)
    }
  }
  console.log(`Vector layers added: ${layersAdded}`)
}

const addRasterLayer = (map) => {
  map.addLayer({
    id: 'raster-layer',
    type: 'raster',
    source: MAP_SOURCE_ID,
    paint: { 'raster-opacity': 1 }
  })
}

export const getPmtilesDiagnostics = async (url, PMTilesImpl = PMTiles) => {
  const archive = new PMTilesImpl(url)
  const header = await archive.getHeader()
  const metadata = await archive.getMetadata()
  const vectorLayers = Array.isArray(metadata?.vector_layers)
    ? metadata.vector_layers.map((layer) => layer.id)
    : []
  console.log('PMTiles diagnostics:', {
    url,
    tileType: header.tileType,
    minZoom: header.minZoom,
    maxZoom: header.maxZoom,
    vectorLayers
  })
  return { header, vectorLayers }
}

const tryVector = (map, pmtilesUrl, header, availableLayers) => {
  try {
    map.addSource(MAP_SOURCE_ID, {
      type: 'vector',
      url: pmtilesUrl,
      minzoom: header?.minZoom ?? 0,
      maxzoom: header?.maxZoom ?? 28
    })
    addVectorLayers(map, availableLayers || [])
    return 'vector'
  } catch (error) {
    console.warn(`Vector load failed: ${error.message}`)
    return null
  }
}

const tryRaster = (map, pmtilesUrl, header) => {
  try {
    map.addSource(MAP_SOURCE_ID, {
      type: 'raster',
      url: pmtilesUrl,
      tileSize: 256,
      minzoom: header?.minZoom ?? 0,
      maxzoom: header?.maxZoom ?? 22
    })
    addRasterLayer(map)
    return 'raster'
  } catch (error) {
    console.warn(`Raster load failed: ${error.message}`)
    return null
  }
}

const loadMbtiles = async (map, filename, fetchImpl) => {
  try {
    let meta = null
    try {
      const res = await fetchImpl(mbtilesMetadataUrl(filename))
      if (res.ok) meta = await res.json()
    } catch (_) { /* use default */ }
    const tileFormat = meta?.format || 'png'
    const tileUrl = mbtilesTileUrl(filename)

    if (isVectorTileFormat(tileFormat)) {
      map.addSource(MAP_SOURCE_ID, { type: 'vector', tiles: [tileUrl], minzoom: 0, maxzoom: 14 })
      addVectorLayers(map, parseMbtilesVectorLayers(meta))
      return 'vector'
    }
    map.addSource(MAP_SOURCE_ID, { type: 'raster', tiles: [tileUrl], tileSize: 256, minzoom: 0, maxzoom: 19 })
    addRasterLayer(map)
    return 'raster'
  } catch (error) {
    console.warn(`MBTiles load failed: ${error.message}`)
    return null
  }
}

/**
 * Load the archive `filename` into `map` as source MAP_SOURCE_ID.
 * PMTiles tile types 1/6 (MVT/MLT) load as vector, 2–5 (PNG/JPEG/WebP/AVIF)
 * as raster, with the other mode as fallback. MBTiles use the tile endpoint.
 * @returns {Promise<'vector'|'raster'|null>} the render mode, or null when nothing could be loaded.
 */
export const loadMapSource = async (map, filename, {
  origin = globalThis.location?.origin,
  fetchImpl = (...args) => globalThis.fetch(...args),
  diagnostics = getPmtilesDiagnostics
} = {}) => {
  if (getMapFormat(filename) === 'mbtiles') return loadMbtiles(map, filename, fetchImpl)

  const rawUrl = buildMapDataUrl(filename, origin)
  const pmtilesUrl = `pmtiles://${rawUrl}`
  let header = null
  let vectorLayers = []
  let tileType = null
  try {
    const d = await diagnostics(rawUrl)
    header = d.header
    vectorLayers = d.vectorLayers
    tileType = d.header?.tileType
  } catch (diagError) {
    console.warn(`PMTiles diagnostics failed, using fallback loading: ${diagError.message}`)
  }

  if (tileType === 1 || tileType === 6) {
    return tryVector(map, pmtilesUrl, header, vectorLayers) || tryRaster(map, pmtilesUrl, header)
  }
  if (tileType >= 2 && tileType <= 5) {
    return tryRaster(map, pmtilesUrl, header) || tryVector(map, pmtilesUrl, header, vectorLayers)
  }
  return tryVector(map, pmtilesUrl, header, vectorLayers) || tryRaster(map, pmtilesUrl, header)
}
