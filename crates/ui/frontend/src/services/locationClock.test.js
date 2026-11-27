import { describe, expect, it } from 'vitest'
import { estimateTimeZoneOffsetMinutes, getLocationClock } from './locationClock.js'

describe('locationClock', () => {
  it('estimates solar time zone offsets from longitude', () => {
    expect(estimateTimeZoneOffsetMinutes(0)).toBe(0)
    expect(estimateTimeZoneOffsetMinutes(15)).toBe(60)
    expect(estimateTimeZoneOffsetMinutes(-75)).toBe(-300)
  })

  it('falls back to system mode without a location', () => {
    const clock = getLocationClock(new Date('2026-06-21T12:00:00Z'), null)
    expect(clock.mode).toBe('system')
    expect(clock.sunriseText).toBeNull()
  })

  it('computes sunrise before sunset for a mid-latitude summer day', () => {
    const clock = getLocationClock(new Date('2026-06-21T12:00:00Z'), { latitude: 59.33, longitude: 18.07 })
    expect(clock.mode).toBe('location')
    expect(clock.sunriseText).toMatch(/^\d{2}:\d{2}$/)
    expect(clock.sunsetText).toMatch(/^\d{2}:\d{2}$/)
    expect(clock.sunriseText < clock.sunsetText).toBe(true)
  })
})
