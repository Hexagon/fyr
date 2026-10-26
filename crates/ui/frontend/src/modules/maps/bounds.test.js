import { describe, expect, it, vi } from 'vitest'
import { createBoundsResolver } from './bounds.js'

const maps = [
  { filename: 'world.pmtiles', size: 1, modified: 'a' },
  { filename: 'sweden.pmtiles', size: 2, modified: 'a' },
  { filename: 'stockholm.mbtiles', size: 3, modified: 'a' },
  { filename: 'broken.pmtiles', size: 4, modified: 'a' },
  { filename: 'nobounds.mbtiles', size: 5, modified: 'a' }
]

const headers = {
  'world.pmtiles': { minLon: -180, minLat: -85.05, maxLon: 180, maxLat: 85.05 },
  'sweden.pmtiles': { minLon: 10.5, minLat: 55, maxLon: 24.2, maxLat: 69.1 }
}

const makeResolver = () => {
  const loadPmtilesHeader = vi.fn(async (filename) => {
    if (filename === 'broken.pmtiles') throw new Error('404')
    return headers[filename]
  })
  const loadMbtilesMetadata = vi.fn(async (filename) => (
    filename === 'stockholm.mbtiles' ? { format: 'png', bounds: '17.5,59.1,18.6,59.6' } : { format: 'png' }
  ))
  return { resolver: createBoundsResolver({ loadPmtilesHeader, loadMbtilesMetadata }), loadPmtilesHeader, loadMbtilesMetadata }
}

describe('createBoundsResolver', () => {
  it('reads PMTiles header bounds and MBTiles metadata bounds', async () => {
    const { resolver } = makeResolver()
    expect(await resolver.getBounds(maps[1])).toEqual([10.5, 55, 24.2, 69.1])
    expect(await resolver.getBounds(maps[2])).toEqual([17.5, 59.1, 18.6, 59.6])
  })

  it('returns null for unreadable archives and missing bounds', async () => {
    const { resolver } = makeResolver()
    expect(await resolver.getBounds(maps[3])).toBeNull()
    expect(await resolver.getBounds(maps[4])).toBeNull()
    expect(await resolver.getBounds(null)).toBeNull()
  })

  it('caches per file version', async () => {
    const { resolver, loadPmtilesHeader } = makeResolver()
    await resolver.getBounds(maps[1])
    await resolver.getBounds(maps[1])
    expect(loadPmtilesHeader).toHaveBeenCalledTimes(1)
    await resolver.getBounds({ ...maps[1], modified: 'b' })
    expect(loadPmtilesHeader).toHaveBeenCalledTimes(2)
  })

  it('picks the most specific map containing a point', async () => {
    const { resolver } = makeResolver()
    expect((await resolver.pickMap(maps, 18.07, 59.33)).filename).toBe('stockholm.mbtiles')
    expect((await resolver.pickMap(maps, 13.0, 55.6)).filename).toBe('sweden.pmtiles')
    expect((await resolver.pickMap(maps, -74, 40.7)).filename).toBe('world.pmtiles')
    expect(await resolver.pickMap(maps.slice(1), -74, 40.7)).toBeNull()
    expect(await resolver.pickMap([], 0, 0)).toBeNull()
  })
})
