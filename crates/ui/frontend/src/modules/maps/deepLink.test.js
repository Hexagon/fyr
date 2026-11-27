import { describe, expect, it } from 'vitest'
import {
  parseMapsQuery,
  buildMapsQuery,
  buildMapsLink,
  deepLinkKey,
  sanitizeLabel,
  DEFAULT_POINT_ZOOM,
  MAX_LABEL_LENGTH
} from './deepLink.js'

describe('parseMapsQuery', () => {
  it('parses a full deep link', () => {
    expect(parseMapsQuery({ map: 'sweden.pmtiles', lat: '59.33', lng: '18.07', zoom: '12', label: 'Camp' })).toEqual({
      map: 'sweden.pmtiles', lat: 59.33, lng: 18.07, zoom: 12, label: 'Camp', hasPoint: true
    })
  })

  it('treats everything as optional', () => {
    expect(parseMapsQuery({})).toEqual({ map: null, lat: null, lng: null, zoom: null, label: null, hasPoint: false })
    expect(parseMapsQuery(undefined).hasPoint).toBe(false)
    expect(parseMapsQuery({ map: 'a.mbtiles' }).map).toBe('a.mbtiles')
  })

  it('requires both lat and lng within range', () => {
    expect(parseMapsQuery({ lat: '59.33' }).hasPoint).toBe(false)
    expect(parseMapsQuery({ lat: '95', lng: '10' }).hasPoint).toBe(false)
    expect(parseMapsQuery({ lat: 'abc', lng: '10' }).hasPoint).toBe(false)
    expect(parseMapsQuery({ lat: '', lng: '' }).hasPoint).toBe(false)
  })

  it('accepts southern/western values, lon/long/z aliases and wraps longitude', () => {
    const p = parseMapsQuery({ lat: '-33.86', lon: '-151.2', z: '9' })
    expect(p).toMatchObject({ lat: -33.86, lng: -151.2, zoom: 9, hasPoint: true })
    expect(parseMapsQuery({ lat: '0', long: '190' }).lng).toBe(-170)
  })

  it('takes the first value of repeated params and clamps zoom', () => {
    expect(parseMapsQuery({ lat: ['1', '2'], lng: ['3'], zoom: '99' })).toMatchObject({ lat: 1, lng: 3, zoom: 22 })
    expect(parseMapsQuery({ zoom: '-4' }).zoom).toBe(0)
  })

  it('sanitises labels', () => {
    expect(parseMapsQuery({ lat: '1', lng: '2', label: '  Water\n source\u0007 ' }).label).toBe('Water source')
    expect(sanitizeLabel('x'.repeat(500))).toHaveLength(MAX_LABEL_LENGTH)
    expect(parseMapsQuery({ label: '   ' }).label).toBeNull()
    // Labels are kept verbatim (rendered as text, never HTML).
    expect(sanitizeLabel('<b>hi</b>')).toBe('<b>hi</b>')
  })
})

describe('buildMapsQuery / buildMapsLink', () => {
  it('omits absent values and rounds numbers', () => {
    expect(buildMapsQuery({})).toEqual({})
    expect(buildMapsQuery({ map: 'a.pmtiles' })).toEqual({ map: 'a.pmtiles' })
    expect(buildMapsQuery({ lat: 59.123456789, lng: 18.987654321, zoom: 12.3456 }))
      .toEqual({ lat: '59.123457', lng: '18.987654', zoom: '12.35' })
  })

  it('only includes a label together with a point', () => {
    expect(buildMapsQuery({ label: 'x' })).toEqual({})
    expect(buildMapsQuery({ lat: 1, lng: 2, label: ' Camp ' }).label).toBe('Camp')
  })

  it('builds encoded relative links', () => {
    expect(buildMapsLink()).toBe('/maps')
    expect(buildMapsLink({ lat: 59.33, lng: 18.07, label: 'Camp & lake' }))
      .toBe('/maps?lat=59.33&lng=18.07&label=Camp+%26+lake')
    expect(buildMapsLink({ map: 'my map.pmtiles', lat: -33.9, lng: -151.2, zoom: 10 }))
      .toBe('/maps?map=my+map.pmtiles&lat=-33.9&lng=-151.2&zoom=10')
  })

  it('round-trips through URLSearchParams', () => {
    const link = buildMapsLink({ map: 'x.mbtiles', lat: -12.5, lng: 130.8, zoom: 8, label: 'Darwin ✓' })
    const params = Object.fromEntries(new URLSearchParams(link.split('?')[1]))
    expect(parseMapsQuery(params)).toEqual({ map: 'x.mbtiles', lat: -12.5, lng: 130.8, zoom: 8, label: 'Darwin ✓', hasPoint: true })
  })

  it('exposes a sensible default point zoom', () => {
    expect(DEFAULT_POINT_ZOOM).toBeGreaterThanOrEqual(10)
  })
})

describe('deepLinkKey', () => {
  it('changes when any parameter changes', () => {
    const a = parseMapsQuery({ lat: '1', lng: '2' })
    const b = parseMapsQuery({ lat: '1', lng: '2', label: 'x' })
    expect(deepLinkKey(a)).toBe(deepLinkKey(parseMapsQuery({ lat: '1.0', lng: '2' })))
    expect(deepLinkKey(a)).not.toBe(deepLinkKey(b))
    expect(deepLinkKey(null)).toBe('')
  })
})
