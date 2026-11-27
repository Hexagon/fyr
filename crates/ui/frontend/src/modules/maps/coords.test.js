import { describe, expect, it } from 'vitest'
import {
  formatDecimal,
  toDmsParts,
  toDms,
  formatDms,
  formatCoordinate,
  parseCoordinate
} from './coords.js'

const close = (result, lat, lng, digits = 4) => {
  expect(result).not.toBeNull()
  expect(result.lat).toBeCloseTo(lat, digits)
  expect(result.lng).toBeCloseTo(lng, digits)
}

describe('formatting', () => {
  it('formats decimal degrees', () => {
    expect(formatDecimal(59.3293, 18.0686)).toBe('59.32930, 18.06860')
    expect(formatDecimal(-33.8688, -151.2093, 3)).toBe('-33.869, -151.209')
  })

  it('splits DMS parts with carry', () => {
    expect(toDmsParts(59.5)).toEqual({ degrees: 59, minutes: 30, seconds: 0 })
    // 10.9999999° must not become 10°59'60.0"
    expect(toDmsParts(10.9999999)).toEqual({ degrees: 11, minutes: 0, seconds: 0 })
    expect(toDmsParts(-0.0166667).minutes).toBe(1)
  })

  it('formats DMS with hemispheres for all quadrants', () => {
    expect(toDms(59.3293, 'lat')).toBe('59°19\'45.5"N')
    expect(toDms(18.0686, 'lng')).toBe('18°04\'07.0"E')
    expect(toDms(-33.8688, 'lat')).toBe('33°52\'07.7"S')
    expect(toDms(-151.2093, 'lng')).toBe('151°12\'33.5"W')
    expect(toDms(0, 'lat')).toBe('0°00\'00.0"N')
    expect(toDms(-0.000001, 'lng')).toBe('0°00\'00.0"E')
    expect(formatDms(59.3293, 18.0686)).toBe('59°19\'45.5"N 18°04\'07.0"E')
  })

  it('dispatches on mode', () => {
    expect(formatCoordinate(1, 2)).toBe(formatDecimal(1, 2))
    expect(formatCoordinate(1, 2, 'dms')).toBe(formatDms(1, 2))
  })
})

describe('parseCoordinate: decimal', () => {
  it('parses comma, space, semicolon and slash separated pairs', () => {
    close(parseCoordinate('59.33, 18.07'), 59.33, 18.07)
    close(parseCoordinate('59.33,18.07'), 59.33, 18.07)
    close(parseCoordinate('  59.33   18.07 '), 59.33, 18.07)
    close(parseCoordinate('-33.86;151.21'), -33.86, 151.21)
    close(parseCoordinate('40.7 / -74.0'), 40.7, -74.0)
    close(parseCoordinate('+12.5, +7'), 12.5, 7)
  })

  it('parses negative (southern/western) values and unicode minus', () => {
    close(parseCoordinate('-33.8688, -151.2093'), -33.8688, -151.2093)
    close(parseCoordinate('−22.9, −43.2'), -22.9, -43.2)
  })

  it('parses decimal commas and geo: URIs', () => {
    close(parseCoordinate('59,3293 18,0686'), 59.3293, 18.0686)
    close(parseCoordinate('geo:59.33,18.07'), 59.33, 18.07)
    close(parseCoordinate('geo:59.33,18.07;u=35'), 59.33, 18.07)
  })

  it('parses hemisphere letters with decimals', () => {
    close(parseCoordinate('59.33N 18.07E'), 59.33, 18.07)
    close(parseCoordinate('33.86 S, 151.21 E'), -33.86, 151.21)
    close(parseCoordinate('N 40.7 W 74.0'), 40.7, -74.0)
    close(parseCoordinate('18.07E 59.33N'), 59.33, 18.07)
    close(parseCoordinate('59.33n, 18.07w'), 59.33, -18.07)
  })
})

describe('parseCoordinate: DMS', () => {
  it('parses DMS with symbols and suffix hemispheres', () => {
    close(parseCoordinate('59°19\'48"N 18°4\'12"E'), 59.33, 18.07)
    close(parseCoordinate('59°19′48″N, 18°04′12″E'), 59.33, 18.07)
    close(parseCoordinate("59°19'48''N 18°4'12''E"), 59.33, 18.07)
    close(parseCoordinate('33°52\'07.7"S 151°12\'33.5"W'), -33.8688, -151.2093)
  })

  it('parses degrees + decimal minutes with prefix hemispheres', () => {
    close(parseCoordinate('N 59° 19.8\' E 18° 4.2\''), 59.33, 18.07)
    close(parseCoordinate('S33 52.13 E151 12.56'), -(33 + 52.13 / 60), 151 + 12.56 / 60)
  })

  it('parses space separated DMS with or without letters', () => {
    close(parseCoordinate('59 19 48 N, 18 4 12 E'), 59.33, 18.07)
    close(parseCoordinate('59 19 48, 18 4 12'), 59.33, 18.07)
    close(parseCoordinate('59 19 48 18 4 12'), 59.33, 18.07)
    close(parseCoordinate('-33 52 7.7, -151 12 33.5'), -33.8688, -151.2093, 3)
  })

  it('parses symbols without letters, negative degrees apply to the whole value', () => {
    close(parseCoordinate('59°19\'48" 18°4\'12"'), 59.33, 18.07)
    close(parseCoordinate('-33°52\'7.7", -151°12\'33.5"'), -33.8688, -151.2093, 3)
    close(parseCoordinate('59.33° 18.07°'), 59.33, 18.07)
  })

  it('round-trips formatDms output for every hemisphere', () => {
    const samples = [
      [59.3293, 18.0686],
      [-33.8688, 151.2093],
      [40.7128, -74.006],
      [-22.9068, -43.1729],
      [0, 0],
      [89.9999, 179.9999],
      [-89.9999, -179.9999]
    ]
    for (const [lat, lng] of samples) {
      // 0.1" ≈ 3 m, so 4 decimals of a degree is the expected precision.
      close(parseCoordinate(formatDms(lat, lng)), lat, lng, 4)
      close(parseCoordinate(formatDecimal(lat, lng)), lat, lng, 5)
    }
  })
})

describe('parseCoordinate: rejects', () => {
  it.each([
    '',
    '   ',
    null,
    undefined,
    'hello',
    'NEW YORK',
    '59.33',
    '1 2 3',
    '91, 0',
    '0, 181',
    '-90.5, 10',
    '59 61 0, 18 0 0', // minutes >= 60
    '59 10 60, 18 0 0', // seconds >= 60
    '59.5 30, 18 0', // fractional degrees with minutes
    'N 10 S 20', // both latitude
    'E 10 W 20', // both longitude
    'S -33, E 151', // contradictory sign + hemisphere
    '59.33N 18.07E 5',
    '1,2,3,4',
    '59,33,18,07',
    '59.33, 18.07 extra'
  ])('rejects %j', (input) => {
    expect(parseCoordinate(input)).toBeNull()
  })

  it('accepts the poles and antimeridian', () => {
    close(parseCoordinate('90, 180'), 90, 180)
    close(parseCoordinate('-90, -180'), -90, -180)
  })

  it('normalises -0 to 0', () => {
    const r = parseCoordinate('-0, -0')
    expect(Object.is(r.lat, 0)).toBe(true)
    expect(Object.is(r.lng, 0)).toBe(true)
  })
})
