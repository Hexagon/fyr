import { describe, expect, it } from 'vitest'
import {
  getMapFormat,
  isMbtiles,
  buildMapDataUrl,
  mbtilesMetadataUrl,
  mbtilesTileUrl,
  isVectorTileFormat,
  parseMbtilesVectorLayers
} from './mapFiles.js'

describe('map file helpers', () => {
  it('detects formats by extension', () => {
    expect(getMapFormat('Region.PMTiles')).toBe('pmtiles')
    expect(getMapFormat('a.b.mbtiles')).toBe('mbtiles')
    expect(isMbtiles('x.MBTILES')).toBe(true)
    expect(isMbtiles('x.pmtiles')).toBe(false)
    expect(getMapFormat(null)).toBe('')
  })

  it('builds encoded URLs', () => {
    expect(buildMapDataUrl('my map#1.pmtiles', 'http://fyr.local:8080'))
      .toBe('http://fyr.local:8080/data/maps/my%20map%231.pmtiles')
    expect(mbtilesMetadataUrl('a b.mbtiles')).toBe('/api/maps/tiles/a%20b.mbtiles/metadata')
    expect(mbtilesTileUrl('a b.mbtiles')).toBe('/api/maps/tiles/a%20b.mbtiles/{z}/{x}/{y}')
  })

  it('recognises vector tile formats', () => {
    expect(isVectorTileFormat('pbf')).toBe(true)
    expect(isVectorTileFormat('MVT')).toBe(true)
    expect(isVectorTileFormat('png')).toBe(false)
    expect(isVectorTileFormat(undefined)).toBe(false)
  })
})

describe('parseMbtilesVectorLayers', () => {
  it('reads vector_layers from the spec "json" metadata row', () => {
    const meta = { format: 'pbf', json: JSON.stringify({ vector_layers: [{ id: 'water' }, { id: 'transportation' }] }) }
    expect(parseMbtilesVectorLayers(meta)).toEqual(['water', 'transportation'])
  })

  it('reads a top-level vector_layers row (string or array)', () => {
    expect(parseMbtilesVectorLayers({ vector_layers: JSON.stringify([{ id: 'roads' }, 'water']) })).toEqual(['roads', 'water'])
    expect(parseMbtilesVectorLayers({ vector_layers: [{ id: 'roads' }] })).toEqual(['roads'])
  })

  it('returns an empty list for missing or malformed metadata', () => {
    expect(parseMbtilesVectorLayers(null)).toEqual([])
    expect(parseMbtilesVectorLayers({ format: 'pbf' })).toEqual([])
    expect(parseMbtilesVectorLayers({ json: '{not json' })).toEqual([])
    expect(parseMbtilesVectorLayers({ vector_layers: '[{"noid":1}]' })).toEqual([])
  })
})
