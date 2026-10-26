import { describe, expect, it } from 'vitest'
import {
  MAP_SOURCE_ID,
  layerGroups,
  defaultLayerVisibility,
  buildBaseStyle,
  resolveSourceLayers,
  buildVectorLayerDefs,
  buildVectorLayers,
  buildMeasureLayers,
  buildPoiLayers,
  poiLayerIds,
  POI_INTERACTIVE_LAYER_IDS,
  TRACK_SOURCE_ID,
  buildTrackLayers
} from './style.js'

describe('style builders', () => {
  it('provides a background-only base style', () => {
    const style = buildBaseStyle()
    expect(style.version).toBe(8)
    expect(style.layers).toHaveLength(1)
    expect(style.layers[0].type).toBe('background')
  })

  it('has a visibility flag for every layer group', () => {
    expect(Object.keys(defaultLayerVisibility()).sort()).toEqual(Object.keys(layerGroups).sort())
  })

  it('every grouped layer id exists in the vector style', () => {
    const ids = new Set(buildVectorLayerDefs([]).map((l) => l.id))
    for (const group of Object.values(layerGroups)) {
      for (const id of group) expect(ids.has(id)).toBe(true)
    }
  })

  it('maps Protomaps-style schemas', () => {
    const m = resolveSourceLayers(['earth', 'water', 'roads', 'landuse', 'buildings', 'places', 'pois', 'boundaries', 'transit'])
    expect(m.earthSource).toBe('earth')
    expect(m.roadsSource).toBe('roads')
    expect(m.waterSource).toBe('water')
    expect(m.placesSource).toBe('places')
  })

  it('maps Shortbread/OpenMapTiles-style schemas', () => {
    const shortbread = resolveSourceLayers(['ocean', 'water_polygons', 'water_lines', 'streets', 'land', 'buildings'])
    expect(shortbread.roadsSource).toBe('streets')
    expect(shortbread.waterLineSource).toBe('water_lines')
    const omt = resolveSourceLayers(['water', 'waterway', 'transportation', 'landcover', 'building', 'place', 'boundary'])
    expect(omt.roadsSource).toBe('transportation')
    expect(omt.waterLineSource).toBe('waterway')
    expect(omt.buildingSource).toBe('building')
  })

  it('attempts all layers when the tileset advertises nothing', () => {
    const { layers, skipped } = buildVectorLayers([])
    expect(skipped).toHaveLength(0)
    expect(layers.length).toBe(buildVectorLayerDefs([]).length)
    expect(layers.every((l) => l.source === MAP_SOURCE_ID)).toBe(true)
    expect(layers.every((l) => l.layout.visibility === 'visible')).toBe(true)
  })

  it('skips layers whose source-layer is missing', () => {
    const { layers, skipped } = buildVectorLayers(['water'], 'src')
    expect(layers.every((l) => l['source-layer'] === 'water')).toBe(true)
    expect(layers.every((l) => l.source === 'src')).toBe(true)
    expect(skipped.length).toBeGreaterThan(0)
  })

  it('merges layout and keeps minzoom/filter/paint', () => {
    const { layers } = buildVectorLayers(['streets'])
    const major = layers.find((l) => l.id === 'road-major-layer')
    expect(major.minzoom).toBe(4)
    expect(major.layout['line-cap']).toBe('round')
    expect(major.layout.visibility).toBe('visible')
    expect(Array.isArray(major.filter)).toBe(true)
  })

  it('builds overlay layers bound to their sources', () => {
    expect(buildMeasureLayers().map((l) => l.source)).toEqual(['measure-source', 'measure-source'])
    expect(buildPoiLayers('x').every((l) => l.source === 'x')).toBe(true)
    const ids = poiLayerIds()
    expect(ids).toEqual(buildPoiLayers().map((l) => l.id).reverse())
  })

  it('renders POI points, lines and polygons with geometry filters', () => {
    const byId = Object.fromEntries(buildPoiLayers().map((l) => [l.id, l]))
    expect(byId['poi-circle-layer'].filter).toEqual(['==', '$type', 'Point'])
    expect(byId['poi-label-layer'].filter).toEqual(['==', '$type', 'Point'])
    expect(byId['poi-line-layer'].type).toBe('line')
    expect(byId['poi-line-layer'].filter).toEqual(['==', '$type', 'LineString'])
    expect(byId['poi-fill-layer'].filter).toEqual(['==', '$type', 'Polygon'])
    for (const id of POI_INTERACTIVE_LAYER_IDS) expect(byId[id]).toBeDefined()
    // Lines are drawn below points so markers stay clickable.
    const order = buildPoiLayers().map((l) => l.id)
    expect(order.indexOf('poi-line-layer')).toBeLessThan(order.indexOf('poi-circle-layer'))
  })

  it('builds the track overlay', () => {
    const [line] = buildTrackLayers()
    expect(line.source).toBe(TRACK_SOURCE_ID)
    expect(line.type).toBe('line')
  })
})
