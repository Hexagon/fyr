import { beforeEach, describe, expect, it, vi } from 'vitest'
import { loadMapSource } from './mapSources.js'
import { MAP_SOURCE_ID } from './style.js'

const fakeMap = () => {
  const map = { sources: {}, layers: [] }
  map.addSource = vi.fn((id, spec) => {
    if (map.sources[id]) throw new Error(`Source ${id} already exists`)
    map.sources[id] = spec
  })
  map.addLayer = vi.fn((layer) => { map.layers.push(layer) })
  return map
}

const jsonResponse = (body, ok = true) => ({ ok, json: async () => body })

beforeEach(() => {
  vi.spyOn(console, 'log').mockImplementation(() => {})
  vi.spyOn(console, 'warn').mockImplementation(() => {})
})

describe('loadMapSource: PMTiles', () => {
  const origin = 'http://fyr.local'

  it('loads vector tilesets (tileType 1) with the advertised layers', async () => {
    const map = fakeMap()
    const diagnostics = vi.fn(async () => ({ header: { tileType: 1, minZoom: 0, maxZoom: 14 }, vectorLayers: ['water', 'roads'] }))
    expect(await loadMapSource(map, 'se.pmtiles', { origin, diagnostics })).toBe('vector')
    expect(diagnostics).toHaveBeenCalledWith('http://fyr.local/data/maps/se.pmtiles')
    expect(map.sources[MAP_SOURCE_ID]).toEqual({ type: 'vector', url: 'pmtiles://http://fyr.local/data/maps/se.pmtiles', minzoom: 0, maxzoom: 14 })
    expect(map.layers.length).toBeGreaterThan(0)
    expect(map.layers.every((l) => ['water', 'roads'].includes(l['source-layer']))).toBe(true)
  })

  it('loads raster tilesets (tileType 2–5)', async () => {
    for (const tileType of [2, 3, 4, 5]) {
      const map = fakeMap()
      const diagnostics = async () => ({ header: { tileType, minZoom: 0, maxZoom: 9 }, vectorLayers: [] })
      expect(await loadMapSource(map, 'r.pmtiles', { origin, diagnostics })).toBe('raster')
      expect(map.sources[MAP_SOURCE_ID]).toMatchObject({ type: 'raster', tileSize: 256, maxzoom: 9 })
      expect(map.layers.map((l) => l.id)).toEqual(['raster-layer'])
    }
  })

  it('falls back to vector when diagnostics fail', async () => {
    const map = fakeMap()
    const diagnostics = async () => { throw new Error('range requests unsupported') }
    expect(await loadMapSource(map, 'x.pmtiles', { origin, diagnostics })).toBe('vector')
    expect(map.sources[MAP_SOURCE_ID]).toMatchObject({ type: 'vector', maxzoom: 28 })
  })

  it('returns null when nothing can be added', async () => {
    const map = fakeMap()
    map.addSource = vi.fn(() => { throw new Error('nope') })
    const diagnostics = async () => ({ header: { tileType: 1 }, vectorLayers: [] })
    expect(await loadMapSource(map, 'x.pmtiles', { origin, diagnostics })).toBeNull()
  })
})

describe('loadMapSource: MBTiles', () => {
  it('loads raster MBTiles through the tile endpoint', async () => {
    const map = fakeMap()
    const fetchImpl = vi.fn(async () => jsonResponse({ format: 'jpg' }))
    expect(await loadMapSource(map, 'a b.mbtiles', { fetchImpl })).toBe('raster')
    expect(fetchImpl).toHaveBeenCalledWith('/api/maps/tiles/a%20b.mbtiles/metadata')
    expect(map.sources[MAP_SOURCE_ID]).toEqual({ type: 'raster', tiles: ['/api/maps/tiles/a%20b.mbtiles/{z}/{x}/{y}'], tileSize: 256, minzoom: 0, maxzoom: 19 })
  })

  it('loads vector MBTiles and filters layers from the json metadata row', async () => {
    const map = fakeMap()
    const fetchImpl = async () => jsonResponse({ format: 'pbf', json: JSON.stringify({ vector_layers: [{ id: 'transportation' }] }) })
    expect(await loadMapSource(map, 'omt.mbtiles', { fetchImpl })).toBe('vector')
    expect(map.sources[MAP_SOURCE_ID].type).toBe('vector')
    expect(map.layers.length).toBeGreaterThan(0)
    expect(map.layers.every((l) => l['source-layer'] === 'transportation')).toBe(true)
  })

  it('defaults to raster PNG when metadata is unavailable', async () => {
    const map = fakeMap()
    const fetchImpl = async () => { throw new Error('offline') }
    expect(await loadMapSource(map, 'x.mbtiles', { fetchImpl })).toBe('raster')
    const map2 = fakeMap()
    expect(await loadMapSource(map2, 'x.mbtiles', { fetchImpl: async () => jsonResponse({}, false) })).toBe('raster')
  })
})
