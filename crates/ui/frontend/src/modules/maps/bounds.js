// Resolve geographic bounds for map archives so deep links can pick a map
// that covers a point. Loaders are injected so this stays testable.

import { boundsFromPmtilesHeader, normalizeBounds, pickMapForPoint } from './geo.js'
import { isMbtiles } from './mapFiles.js'

/**
 * @param {object} loaders
 * @param {(filename: string) => Promise<object>} loaders.loadPmtilesHeader  resolves a PMTiles v3 header
 * @param {(filename: string) => Promise<object>} loaders.loadMbtilesMetadata resolves MBTiles metadata (name → value)
 */
export const createBoundsResolver = ({ loadPmtilesHeader, loadMbtilesMetadata }) => {
  const cache = new Map()

  const cacheKey = (map) => `${map.filename}|${map.size ?? ''}|${map.modified ?? ''}`

  const load = async (map) => {
    try {
      if (isMbtiles(map.filename)) {
        const meta = await loadMbtilesMetadata(map.filename)
        return normalizeBounds(meta?.bounds)
      }
      return boundsFromPmtilesHeader(await loadPmtilesHeader(map.filename))
    } catch {
      return null
    }
  }

  /** Bounds [w, s, e, n] for a map list entry, or null when unknown. Cached per file version. */
  const getBounds = (map) => {
    if (!map?.filename) return Promise.resolve(null)
    const key = cacheKey(map)
    if (!cache.has(key)) cache.set(key, load(map))
    return cache.get(key)
  }

  /** Pick the most specific map covering the point, or null. */
  const pickMap = async (maps, lng, lat) => {
    const entries = await Promise.all((maps || []).map(async (map) => ({ map, bounds: await getBounds(map) })))
    return pickMapForPoint(entries, lng, lat)
  }

  return { getBounds, pickMap, clear: () => cache.clear() }
}
