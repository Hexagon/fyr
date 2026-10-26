// Coordinate formatting (decimal / DMS) and free-text parsing.
// All public functions take/return latitude before longitude, as people write them.

const clampDigits = (digits) => Math.max(0, Math.min(10, Number.isInteger(digits) ? digits : 5))

/** "59.32930, 18.06860" */
export const formatDecimal = (lat, lng, digits = 5) => {
  const d = clampDigits(digits)
  return `${Number(lat).toFixed(d)}, ${Number(lng).toFixed(d)}`
}

/**
 * Split an absolute decimal degree value into degrees/minutes/seconds,
 * rounding seconds to `secondDigits` and carrying 60" → 1' and 60' → 1°.
 */
export const toDmsParts = (value, secondDigits = 1) => {
  const abs = Math.abs(Number(value))
  const factor = Math.pow(10, secondDigits)
  // Work in rounded tenths (etc.) of a second to avoid 59.99999" artefacts.
  let totalUnits = Math.round(abs * 3600 * factor)
  const unitsPerDegree = 3600 * factor
  const unitsPerMinute = 60 * factor
  const degrees = Math.floor(totalUnits / unitsPerDegree)
  totalUnits -= degrees * unitsPerDegree
  const minutes = Math.floor(totalUnits / unitsPerMinute)
  totalUnits -= minutes * unitsPerMinute
  const seconds = totalUnits / factor
  return { degrees, minutes, seconds }
}

/** One axis in DMS: 59°19'45.5"N */
export const toDms = (value, axis, secondDigits = 1) => {
  const v = Number(value)
  const { degrees, minutes, seconds } = toDmsParts(v, secondDigits)
  const isZero = degrees === 0 && minutes === 0 && seconds === 0
  const hemisphere = axis === 'lat'
    ? (v < 0 && !isZero ? 'S' : 'N')
    : (v < 0 && !isZero ? 'W' : 'E')
  const secText = seconds.toFixed(secondDigits).padStart(secondDigits > 0 ? secondDigits + 3 : 2, '0')
  return `${degrees}°${String(minutes).padStart(2, '0')}'${secText}"${hemisphere}`
}

/** 59°19'45.5"N 18°04'07.0"E */
export const formatDms = (lat, lng, secondDigits = 1) => `${toDms(lat, 'lat', secondDigits)} ${toDms(lng, 'lng', secondDigits)}`

/** Format using 'decimal' (default) or 'dms'. */
export const formatCoordinate = (lat, lng, mode = 'decimal') =>
  mode === 'dms' ? formatDms(lat, lng) : formatDecimal(lat, lng)

// ── Parsing ─────────────────────────────────────────────────────────

const NUM = '[-+]?\\d+(?:\\.\\d+)?'

const normalizeInput = (text) => String(text ?? '')
  .trim()
  .replace(/^geo:/i, '')
  .replace(/;.*$/, (m) => (/[a-z]=/i.test(m) ? '' : m)) // geo: URI parameters (;u=35)
  .replace(/[º˚]/g, '°')
  .replace(/[′’‘´`]/g, "'")
  .replace(/[″“”]/g, '"')
  .replace(/''/g, '"')
  .replace(/[−–—]/g, '-')
  .replace(/\s+/g, ' ')
  .trim()

/** deg [min [sec]] → decimal degrees, with validation. Returns NaN when invalid. */
const dmsToDecimal = (numbers) => {
  if (numbers.length < 1 || numbers.length > 3) return NaN
  const values = numbers.map(Number)
  if (!values.every(Number.isFinite)) return NaN
  const [deg, min = 0, sec = 0] = values
  const negative = String(numbers[0]).trim().startsWith('-')
  if (numbers.length > 1) {
    // Minutes/seconds only make sense with whole degrees (and whole minutes before seconds).
    if (!Number.isInteger(deg)) return NaN
    if (min < 0 || min >= 60 || sec < 0 || sec >= 60) return NaN
    if (numbers.length > 2 && !Number.isInteger(min)) return NaN
  }
  const magnitude = Math.abs(deg) + min / 60 + sec / 3600
  return negative ? -magnitude : magnitude
}

const finish = (lat, lng) => {
  if (!Number.isFinite(lat) || !Number.isFinite(lng)) return null
  if (lat < -90 || lat > 90 || lng < -180 || lng > 180) return null
  // Avoid -0 leaking into the UI.
  return { lat: lat === 0 ? 0 : lat, lng: lng === 0 ? 0 : lng }
}

const numbersIn = (chunk) => chunk.match(new RegExp(NUM, 'g')) || []

/** Parse a component with a hemisphere letter: { axis: 'lat'|'lng', value } or null. */
const parseHemisphereComponent = (chunk, letter) => {
  const nums = numbersIn(chunk)
  if (nums.some((n) => /^[-+]/.test(n))) return null // "S -33" is contradictory
  const value = dmsToDecimal(nums)
  if (!Number.isFinite(value)) return null
  const upper = letter.toUpperCase()
  const axis = upper === 'N' || upper === 'S' ? 'lat' : 'lng'
  const sign = upper === 'S' || upper === 'W' ? -1 : 1
  return { axis, value: sign * value }
}

const parseWithHemispheres = (text) => {
  const letters = text.match(/[NSEW]/gi) || []
  if (letters.length !== 2) return null
  const firstLetterIdx = text.search(/[NSEW]/i)
  const firstDigitIdx = text.search(/\d/)
  const prefixStyle = firstLetterIdx < firstDigitIdx

  // Split into two chunks, each owning exactly one letter.
  const re = prefixStyle
    ? /([NSEW])([^NSEW]*)/gi
    : /([^NSEW]*)([NSEW])/gi
  const parts = []
  let m
  while ((m = re.exec(text)) !== null) {
    if (m[0] === '') { re.lastIndex++; continue }
    parts.push(prefixStyle ? { letter: m[1], chunk: m[2] } : { letter: m[2], chunk: m[1] })
  }
  if (parts.length !== 2) return null
  // Any trailing text after the last suffix letter must be empty/separators.
  if (!prefixStyle && /\d/.test(text.slice(text.search(/[NSEW][^NSEW]*$/i) + 1))) return null

  const comps = parts.map((p) => parseHemisphereComponent(p.chunk.replace(/^[\s,;/]+|[\s,;/]+$/g, ''), p.letter))
  if (comps.some((c) => !c)) return null
  const lat = comps.find((c) => c.axis === 'lat')
  const lng = comps.find((c) => c.axis === 'lng')
  if (!lat || !lng) return null // e.g. "N 10 S 20"
  return finish(lat.value, lng.value)
}

const parseWithSymbols = (text) => {
  // Each component starts with a number followed by a degree sign.
  const comp = new RegExp(`(${NUM})\\s*°\\s*(?:(${NUM})\\s*'\\s*)?(?:(${NUM})\\s*"\\s*)?`, 'g')
  const matches = [...text.matchAll(comp)]
  if (matches.length !== 2) return null
  const leftover = text.replace(comp, '').replace(/[\s,;/]/g, '')
  if (leftover) return null
  const values = matches.map((mm) => dmsToDecimal([mm[1], mm[2], mm[3]].filter((v) => v !== undefined)))
  return finish(values[0], values[1])
}

const parsePlainNumbers = (text) => {
  if (/[^\d\s.,;+\-/]/.test(text)) return null

  // European decimal commas: "59,3293 18,0686"
  const euro = text.match(new RegExp(`^([-+]?\\d+,\\d+)\\s*[ ;/]\\s*([-+]?\\d+,\\d+)$`))
  if (euro) {
    return finish(Number(euro[1].replace(',', '.')), Number(euro[2].replace(',', '.')))
  }

  const groups = text.split(/\s*[,;/]\s*/).filter((g) => g.trim() !== '')
  if (groups.length === 2) {
    const a = numbersIn(groups[0])
    const b = numbersIn(groups[1])
    if (a.length !== b.length) return null
    return finish(dmsToDecimal(a), dmsToDecimal(b))
  }
  if (groups.length !== 1) return null

  const nums = numbersIn(groups[0])
  if (nums.length === 2 || nums.length === 4 || nums.length === 6) {
    const half = nums.length / 2
    return finish(dmsToDecimal(nums.slice(0, half)), dmsToDecimal(nums.slice(half)))
  }
  return null
}

/**
 * Parse a free-text coordinate. Supported examples:
 *   "59.33, 18.07"   "59.33 18.07"   "-33.86;151.21"   "geo:59.33,18.07"
 *   "59,33 18,07" (decimal commas)
 *   "59°19'48\"N 18°4'12\"E"   "N 59° 19.8' E 18° 4.2'"   "59.33N 18.07E"
 *   "33°52'S, 151°12'E"   "59 19 48 N, 18 4 12 E"   "59°19'48\" 18°4'12\""
 * Returns { lat, lng } or null.
 */
export const parseCoordinate = (input) => {
  const text = normalizeInput(input)
  if (!text) return null
  if (/[NSEW]/i.test(text)) {
    // Reject words that merely contain those letters ("NEW YORK").
    if (/[A-Z]{2,}/i.test(text.replace(/[^A-Za-z]+/g, ' ').split(' ').filter((w) => w.length > 1).join(''))) return null
    return parseWithHemispheres(text)
  }
  if (/[a-z]/i.test(text)) return null
  if (/[°'"]/.test(text)) return parseWithSymbols(text)
  return parsePlainNumbers(text)
}
