const toRadians = (degrees) => degrees * Math.PI / 180
const toDegrees = (radians) => radians * 180 / Math.PI

const normalizeDegrees = (degrees) => {
  const result = degrees % 360
  return result < 0 ? result + 360 : result
}

const normalizeHours = (hours) => {
  const result = hours % 24
  return result < 0 ? result + 24 : result
}

export const estimateTimeZoneOffsetMinutes = (longitude) => {
  const offsetHours = Math.max(-12, Math.min(14, Math.round(longitude / 15)))
  return offsetHours * 60
}

export const formatInOffset = (date, offsetMinutes, options) => {
  const shifted = new Date(date.getTime() + offsetMinutes * 60_000)
  return new Intl.DateTimeFormat(undefined, {
    timeZone: 'UTC',
    ...options
  }).format(shifted)
}

const formatMinutesAsClock = (minutes) => {
  if (minutes === null || minutes === undefined || Number.isNaN(minutes)) {
    return null
  }

  const rounded = Math.round(normalizeHours(minutes / 60) * 60)
  const hours = Math.floor(rounded / 60) % 24
  const mins = rounded % 60
  return `${String(hours).padStart(2, '0')}:${String(mins).padStart(2, '0')}`
}

const dayOfYearInOffset = (date, offsetMinutes) => {
  const shifted = new Date(date.getTime() + offsetMinutes * 60_000)
  const startOfYear = Date.UTC(shifted.getUTCFullYear(), 0, 1)
  const startOfToday = Date.UTC(shifted.getUTCFullYear(), shifted.getUTCMonth(), shifted.getUTCDate())
  return Math.floor((startOfToday - startOfYear) / 86_400_000) + 1
}

const calculateSunEventMinutes = (date, latitude, longitude, offsetMinutes, isSunrise) => {
  const dayOfYear = dayOfYearInOffset(date, offsetMinutes)
  const lngHour = longitude / 15
  const approxTime = dayOfYear + ((isSunrise ? 6 : 18) - lngHour) / 24
  const meanAnomaly = 0.9856 * approxTime - 3.289

  let sunLongitude = meanAnomaly + 1.916 * Math.sin(toRadians(meanAnomaly)) + 0.020 * Math.sin(toRadians(2 * meanAnomaly)) + 282.634
  sunLongitude = normalizeDegrees(sunLongitude)

  let rightAscension = toDegrees(Math.atan(0.91764 * Math.tan(toRadians(sunLongitude))))
  rightAscension = normalizeDegrees(rightAscension)

  const longitudeQuadrant = Math.floor(sunLongitude / 90) * 90
  const rightAscensionQuadrant = Math.floor(rightAscension / 90) * 90
  rightAscension += longitudeQuadrant - rightAscensionQuadrant
  rightAscension /= 15

  const sinDeclination = 0.39782 * Math.sin(toRadians(sunLongitude))
  const cosDeclination = Math.cos(Math.asin(sinDeclination))
  const zenith = toRadians(90.833)

  const cosHourAngle = (
    Math.cos(zenith) - (sinDeclination * Math.sin(toRadians(latitude)))
  ) / (cosDeclination * Math.cos(toRadians(latitude)))

  if (cosHourAngle > 1 || cosHourAngle < -1) {
    return null
  }

  let hourAngle = isSunrise
    ? 360 - toDegrees(Math.acos(cosHourAngle))
    : toDegrees(Math.acos(cosHourAngle))
  hourAngle /= 15

  const localMeanTime = hourAngle + rightAscension - (0.06571 * approxTime) - 6.622
  const utcHours = normalizeHours(localMeanTime - lngHour)

  return utcHours * 60
}

export const getLocationClock = (now, location) => {
  if (!location) {
    return {
      mode: 'system',
      timeText: now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
      dateText: now.toLocaleDateString([], { weekday: 'short', year: 'numeric', month: 'short', day: 'numeric' }),
      sunriseText: null,
      sunsetText: null,
      offsetMinutes: 0
    }
  }

  const offsetMinutes = estimateTimeZoneOffsetMinutes(location.longitude)
  const timeText = formatInOffset(now, offsetMinutes, { hour: '2-digit', minute: '2-digit' })
  const dateText = formatInOffset(now, offsetMinutes, { weekday: 'short', year: 'numeric', month: 'short', day: 'numeric' })
  const sunriseMinutes = calculateSunEventMinutes(now, location.latitude, location.longitude, offsetMinutes, true)
  const sunsetMinutes = calculateSunEventMinutes(now, location.latitude, location.longitude, offsetMinutes, false)

  return {
    mode: 'location',
    timeText,
    dateText,
    sunriseText: formatMinutesAsClock(sunriseMinutes),
    sunsetText: formatMinutesAsClock(sunsetMinutes),
    offsetMinutes
  }
}