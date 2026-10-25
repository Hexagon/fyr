// URL and metadata helpers for map archives (PMTiles / MBTiles).

export const getMapFormat = (filename) => {
  const ext = String(filename || '').split('.').pop().toLowerCase()
  return ext
}

export const isMbtiles = (filename) => getMapFormat(filename) === 'mbtiles'

/** Absolute URL of a raw map archive under /data/maps. */
export const buildMapDataUrl = (filename, origin) => {
  const encodedFilename = encodeURIComponent(String(filename || ''))
  return new URL(`/data/maps/${encodedFilename}`, origin).toString()
}

export const mbtilesMetadataUrl = (filename) =>
  `/api/maps/tiles/${encodeURIComponent(filename)}/metadata`

export const mbtilesTileUrl = (filename) =>
  `/api/maps/tiles/${encodeURIComponent(filename)}/{z}/{x}/{y}`

const VECTOR_FORMATS = new Set(['pbf', 'mvt'])

export const isVectorTileFormat = (format) => VECTOR_FORMATS.has(String(format || '').toLowerCase())

const layerIdsFrom = (list) => (Array.isArray(list)
  ? list.map((l) => (l && typeof l === 'object' ? l.id : l)).filter((id) => typeof id === 'string' && id)
  : [])

const parseMaybeJson = (value) => {
  if (typeof value !== 'string' || !value) return value
  try {
    return JSON.parse(value)
  } catch {
    return null
  }
}

/**
 * Extract vector layer ids from MBTiles metadata. The MBTiles spec stores
 * `vector_layers` inside the JSON-encoded `json` row; some tools also write a
 * top-level `vector_layers` row. Both are strings in Fyr's metadata endpoint.
 */
export const parseMbtilesVectorLayers = (meta) => {
  if (!meta || typeof meta !== 'object') return []
  const direct = layerIdsFrom(parseMaybeJson(meta.vector_layers))
  if (direct.length) return direct
  const json = parseMaybeJson(meta.json)
  if (json && typeof json === 'object') return layerIdsFrom(json.vector_layers)
  return []
}
