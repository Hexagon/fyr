import { describe, expect, it } from 'vitest'
import {
  emptyGeoJSON,
  normalizePoiData,
  normalizePoiFilename,
  slugifyFilename,
  uniquePoiFilename,
  makePointFeature,
  buildMeasureGeoJSON
} from './poi.js'

describe('POI helpers', () => {
  it('creates empty feature collections', () => {
    expect(emptyGeoJSON()).toEqual({ type: 'FeatureCollection', features: [] })
    expect(emptyGeoJSON()).not.toBe(emptyGeoJSON())
  })

  it('normalizes loaded POI data', () => {
    const fc = { type: 'FeatureCollection', features: [makePointFeature([1, 2], 'a')] }
    expect(normalizePoiData(fc)).toBe(fc)
    expect(normalizePoiData({}).features).toEqual([])
    expect(normalizePoiData(null).features).toEqual([])
    expect(normalizePoiData('garbage').features).toEqual([])
    const single = makePointFeature([1, 2], 'a')
    expect(normalizePoiData(single).features).toEqual([single])
    expect(normalizePoiData({ features: [] }).type).toBe('FeatureCollection')
  })

  it('normalizes filenames', () => {
    expect(normalizePoiFilename(' camps ')).toBe('camps.geojson')
    expect(normalizePoiFilename('camps.geojson')).toBe('camps.geojson')
    expect(normalizePoiFilename('camps.JSON')).toBe('camps.JSON')
    expect(normalizePoiFilename('   ')).toBe('')
  })

  it('slugifies and de-duplicates filenames', () => {
    expect(slugifyFilename('Kungsleden Trail.gpx')).toBe('Kungsleden-Trail')
    expect(slugifyFilename('Åre fjäll')).toBe('Are-fjall')
    expect(slugifyFilename('../../etc/passwd')).toBe('etc-passwd')
    expect(slugifyFilename('???')).toBe('import')
    expect(uniquePoiFilename('trail.gpx', [])).toBe('trail.geojson')
    expect(uniquePoiFilename('trail.gpx', ['trail.geojson', 'Trail-2.geojson'])).toBe('trail-3.geojson')
  })

  it('builds point features', () => {
    expect(makePointFeature([18, 59], 'Camp', { sym: 'Tent' })).toEqual({
      type: 'Feature',
      geometry: { type: 'Point', coordinates: [18, 59] },
      properties: { name: 'Camp', sym: 'Tent' }
    })
  })

  it('builds measure GeoJSON', () => {
    expect(buildMeasureGeoJSON([]).features).toHaveLength(0)
    expect(buildMeasureGeoJSON([[0, 0]]).features.map((f) => f.geometry.type)).toEqual(['Point'])
    expect(buildMeasureGeoJSON([[0, 0], [1, 1]]).features.map((f) => f.geometry.type)).toEqual(['LineString', 'Point', 'Point'])
  })
})
