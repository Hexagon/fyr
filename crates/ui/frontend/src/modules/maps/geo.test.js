import { describe, expect, it } from 'vitest'
import {
  haversineDistance,
  pathLength,
  formatDistance,
  formatDuration,
  formatBytes,
  normalizeLng,
  isValidLatLng,
  normalizeBounds,
  boundsFromPmtilesHeader,
  boundsContain,
  boundsArea,
  boundsCenter,
  pickMapForPoint
} from './geo.js'

const STOCKHOLM = [18.0686, 59.3293]
const GOTHENBURG = [11.9746, 57.7089]

describe('haversineDistance', () => {
  it('is zero for identical points', () => {
    expect(haversineDistance(STOCKHOLM, STOCKHOLM)).toBe(0)
  })

  it('matches the known Stockholm–Gothenburg great-circle distance (~398 km)', () => {
    const d = haversineDistance(STOCKHOLM, GOTHENBURG)
    expect(d).toBeGreaterThan(395)
    expect(d).toBeLessThan(400)
  })

  it('is symmetric', () => {
    expect(haversineDistance(STOCKHOLM, GOTHENBURG)).toBeCloseTo(haversineDistance(GOTHENBURG, STOCKHOLM), 10)
  })

  it('measures one degree of latitude as ~111.2 km', () => {
    expect(haversineDistance([0, 0], [0, 1])).toBeCloseTo(111.19, 1)
  })

  it('handles the antimeridian and southern/western hemispheres', () => {
    expect(haversineDistance([179.5, -10], [-179.5, -10])).toBeCloseTo(109.5, 0)
    expect(haversineDistance([-70, -33], [-70, -34])).toBeCloseTo(111.19, 1)
  })

  it('does not return NaN for antipodal points', () => {
    const d = haversineDistance([0, 0], [180, 0])
    expect(Number.isNaN(d)).toBe(false)
    expect(d).toBeCloseTo(Math.PI * 6371, 0)
  })
})

describe('pathLength', () => {
  it('sums segment lengths', () => {
    const pts = [[0, 0], [0, 1], [0, 2]]
    expect(pathLength(pts)).toBeCloseTo(2 * 111.19, 1)
  })

  it('is zero for empty, single and invalid input', () => {
    expect(pathLength([])).toBe(0)
    expect(pathLength([[1, 2]])).toBe(0)
    expect(pathLength(null)).toBe(0)
  })
})

describe('formatting', () => {
  it('formats distances in metres below one kilometre', () => {
    expect(formatDistance(0)).toBe('0 m')
    expect(formatDistance(0.4567)).toBe('457 m')
    expect(formatDistance(1)).toBe('1.00 km')
    expect(formatDistance(12.345)).toBe('12.35 km')
  })

  it('formats durations', () => {
    expect(formatDuration(0)).toBe('0:00')
    expect(formatDuration(65_000)).toBe('1:05')
    expect(formatDuration(3_723_000)).toBe('1:02:03')
    expect(formatDuration(-5)).toBe('0:00')
    expect(formatDuration(undefined)).toBe('0:00')
  })

  it('formats byte sizes', () => {
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes(1536)).toBe('1.5 KB')
    expect(formatBytes(5 * 1024 * 1024)).toBe('5 MB')
  })
})

describe('coordinates validity', () => {
  it('wraps longitudes into [-180, 180]', () => {
    expect(normalizeLng(10)).toBe(10)
    expect(normalizeLng(190)).toBe(-170)
    expect(normalizeLng(-190)).toBe(170)
    expect(normalizeLng(540)).toBe(-180)
    expect(Number.isNaN(normalizeLng('x'))).toBe(true)
  })

  it('validates lat/lng ranges', () => {
    expect(isValidLatLng(59.3, 18.1)).toBe(true)
    expect(isValidLatLng(-90, -180)).toBe(true)
    expect(isValidLatLng(91, 0)).toBe(false)
    expect(isValidLatLng(0, 181)).toBe(false)
    expect(isValidLatLng(NaN, 0)).toBe(false)
  })
})

describe('bounds', () => {
  const sweden = [10.5, 55.0, 24.2, 69.1]

  it('normalizes arrays and MBTiles "w,s,e,n" strings', () => {
    expect(normalizeBounds(sweden)).toEqual(sweden)
    expect(normalizeBounds('10.5, 55.0,24.2,69.1')).toEqual(sweden)
  })

  it('rejects malformed, degenerate and out-of-range bounds', () => {
    expect(normalizeBounds(null)).toBeNull()
    expect(normalizeBounds('1,2,3')).toBeNull()
    expect(normalizeBounds('a,b,c,d')).toBeNull()
    expect(normalizeBounds('1,,3,4')).toBeNull()
    expect(normalizeBounds([0, 0, 0, 0])).toBeNull()
    expect(normalizeBounds([0, 10, 5, 5])).toBeNull()
    expect(normalizeBounds([-200, 0, 10, 10])).toBeNull()
    expect(normalizeBounds([0, -95, 10, 10])).toBeNull()
  })

  it('reads PMTiles header bounds', () => {
    expect(boundsFromPmtilesHeader({ minLon: 10.5, minLat: 55, maxLon: 24.2, maxLat: 69.1 })).toEqual(sweden)
    expect(boundsFromPmtilesHeader({ minLon: 0, minLat: 0, maxLon: 0, maxLat: 0 })).toBeNull()
    expect(boundsFromPmtilesHeader(null)).toBeNull()
  })

  it('tests containment including edges and outside points', () => {
    expect(boundsContain(sweden, 18.07, 59.33)).toBe(true)
    expect(boundsContain(sweden, 10.5, 55.0)).toBe(true)
    expect(boundsContain(sweden, 2.35, 48.85)).toBe(false)
    expect(boundsContain(sweden, 18.07, 70)).toBe(false)
    expect(boundsContain('10.5,55,24.2,69.1', 18.07, 59.33)).toBe(true)
    expect(boundsContain(null, 18.07, 59.33)).toBe(false)
  })

  it('handles southern/western bounds', () => {
    const chile = [-75.7, -56, -66.4, -17.5]
    expect(boundsContain(chile, -70.66, -33.45)).toBe(true)
    expect(boundsContain(chile, 70.66, -33.45)).toBe(false)
    expect(boundsContain(chile, -70.66, 33.45)).toBe(false)
  })

  it('handles bounds crossing the antimeridian', () => {
    const fiji = [176, -21, -178, -12]
    expect(boundsContain(fiji, 178.4, -18.1)).toBe(true)
    expect(boundsContain(fiji, -179, -18.1)).toBe(true)
    expect(boundsContain(fiji, 0, -18.1)).toBe(false)
    expect(boundsArea(fiji)).toBeCloseTo(6 * 9)
    expect(boundsCenter(fiji)[0]).toBeCloseTo(179)
  })

  it('computes area and center', () => {
    expect(boundsArea([0, 0, 10, 5])).toBe(50)
    expect(boundsArea(null)).toBe(Infinity)
    expect(boundsCenter([0, 0, 10, 4])).toEqual([5, 2])
    expect(boundsCenter('bad')).toBeNull()
  })
})

describe('pickMapForPoint', () => {
  const world = { filename: 'world.pmtiles' }
  const sweden = { filename: 'sweden.pmtiles' }
  const stockholm = { filename: 'stockholm.mbtiles' }
  const unknown = { filename: 'unknown.pmtiles' }
  const entries = [
    { map: world, bounds: [-180, -85.05, 180, 85.05] },
    { map: unknown, bounds: null },
    { map: sweden, bounds: [10.5, 55, 24.2, 69.1] },
    { map: stockholm, bounds: '17.5,59.1,18.6,59.6' }
  ]

  it('prefers the smallest map containing the point', () => {
    expect(pickMapForPoint(entries, 18.07, 59.33)).toBe(stockholm)
    expect(pickMapForPoint(entries, 13.0, 55.6)).toBe(sweden)
    expect(pickMapForPoint(entries, -74, 40.7)).toBe(world)
  })

  it('returns null when nothing contains the point', () => {
    expect(pickMapForPoint(entries.slice(1), -74, 40.7)).toBeNull()
    expect(pickMapForPoint([], 0, 0)).toBeNull()
    expect(pickMapForPoint(undefined, 0, 0)).toBeNull()
  })
})
