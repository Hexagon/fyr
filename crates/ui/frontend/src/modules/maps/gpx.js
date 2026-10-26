// Client-side GPX 1.0/1.1 import (→ GeoJSON) and export (GeoJSON → GPX 1.1).
//
// Import mapping:
//   <wpt>            → Point      (properties: name, desc, cmt, sym, type, time, gpxType: 'wpt')
//   <trk>/<trkseg>   → LineString (one feature per segment; properties.coordTimes)
//   <rte>/<rtept>    → LineString (gpxType: 'rte')
// Elevation (<ele>) becomes the third coordinate value.
// Elements are matched by local name so default and prefixed namespaces work.

const GPX_NS = 'http://www.topografix.com/GPX/1/1'

// ── DOM helpers (namespace agnostic) ────────────────────────────────

const localNameOf = (node) => String(node.localName || node.nodeName || '').replace(/^.*:/, '').toLowerCase()

const childElements = (node, name) => {
  const out = []
  for (const child of Array.from(node?.childNodes || [])) {
    if (child.nodeType === 1 && localNameOf(child) === name) out.push(child)
  }
  return out
}

const firstChild = (node, name) => childElements(node, name)[0] || null

const childText = (node, name) => {
  const el = firstChild(node, name)
  if (!el) return null
  const text = String(el.textContent || '').trim()
  return text === '' ? null : text
}

const parseNumberAttr = (el, name) => {
  const raw = el.getAttribute(name)
  if (raw === null || String(raw).trim() === '') return NaN
  return Number(raw)
}

/** Parse a <wpt>/<trkpt>/<rtept>: { position: [lon, lat, ele?], time } or null when invalid. */
const parsePoint = (el) => {
  const lat = parseNumberAttr(el, 'lat')
  const lon = parseNumberAttr(el, 'lon')
  if (!Number.isFinite(lat) || !Number.isFinite(lon) || lat < -90 || lat > 90 || lon < -180 || lon > 180) {
    return null
  }
  const position = [lon, lat]
  const eleText = childText(el, 'ele')
  const ele = eleText === null ? NaN : Number(eleText)
  if (Number.isFinite(ele)) position.push(ele)
  return { position, time: childText(el, 'time') }
}

const descriptiveProps = (el) => {
  const props = {}
  for (const key of ['name', 'desc', 'cmt', 'sym', 'type', 'src']) {
    const value = childText(el, key)
    if (value !== null) props[key] = value
  }
  return props
}

const lineFeature = (points, properties) => {
  const props = { ...properties }
  if (points.some((p) => p.time)) props.coordTimes = points.map((p) => p.time || null)
  return {
    type: 'Feature',
    geometry: { type: 'LineString', coordinates: points.map((p) => p.position) },
    properties: props
  }
}

/**
 * Parse GPX text into GeoJSON features.
 * @returns {{ name: string|null, features: object[], counts: { waypoints: number, tracks: number, routes: number, skippedPoints: number } }}
 * @throws Error when the text is not well-formed GPX.
 */
export const parseGpx = (text) => {
  if (typeof DOMParser === 'undefined') throw new Error('GPX import needs a browser with DOMParser')
  const doc = new DOMParser().parseFromString(String(text ?? ''), 'application/xml')
  if (doc.getElementsByTagName('parsererror').length > 0) {
    throw new Error('The file is not valid XML')
  }
  const root = doc.documentElement
  if (!root || localNameOf(root) !== 'gpx') {
    throw new Error('The file is not a GPX document')
  }

  const features = []
  const counts = { waypoints: 0, tracks: 0, routes: 0, skippedPoints: 0 }
  const metadata = firstChild(root, 'metadata')
  const name = (metadata && childText(metadata, 'name')) || childText(root, 'name') || null

  childElements(root, 'wpt').forEach((wpt) => {
    const point = parsePoint(wpt)
    if (!point) { counts.skippedPoints++; return }
    counts.waypoints++
    const props = descriptiveProps(wpt)
    if (!props.name) props.name = `Waypoint ${counts.waypoints}`
    if (point.time) props.time = point.time
    props.gpxType = 'wpt'
    features.push({ type: 'Feature', geometry: { type: 'Point', coordinates: point.position }, properties: props })
  })

  childElements(root, 'trk').forEach((trk) => {
    const trkProps = descriptiveProps(trk)
    const segments = childElements(trk, 'trkseg')
      .map((seg) => childElements(seg, 'trkpt').map((pt) => {
        const parsed = parsePoint(pt)
        if (!parsed) counts.skippedPoints++
        return parsed
      }).filter(Boolean))
      .filter((pts) => pts.length >= 2)
    if (segments.length === 0) return
    counts.tracks++
    const baseName = trkProps.name || `Track ${counts.tracks}`
    segments.forEach((pts, idx) => {
      const props = { ...trkProps, name: segments.length > 1 ? `${baseName} (${idx + 1}/${segments.length})` : baseName, gpxType: 'trk' }
      if (segments.length > 1) props.segment = idx + 1
      features.push(lineFeature(pts, props))
    })
  })

  childElements(root, 'rte').forEach((rte) => {
    const pts = childElements(rte, 'rtept').map((pt) => {
      const parsed = parsePoint(pt)
      if (!parsed) counts.skippedPoints++
      return parsed
    }).filter(Boolean)
    if (pts.length < 2) return
    counts.routes++
    const props = descriptiveProps(rte)
    if (!props.name) props.name = `Route ${counts.routes}`
    props.gpxType = 'rte'
    features.push(lineFeature(pts, props))
  })

  return { name, features, counts }
}

// ── Export ──────────────────────────────────────────────────────────

// Characters that are not allowed anywhere in an XML 1.0 document.
// eslint-disable-next-line no-control-regex
const INVALID_XML_CHARS = /[^\u0009\u000A\u000D -퟿-�\u{10000}-\u{10FFFF}]/gu

export const escapeXml = (value) => String(value ?? '')
  .replace(INVALID_XML_CHARS, '')
  .replace(/&/g, '&amp;')
  .replace(/</g, '&lt;')
  .replace(/>/g, '&gt;')
  .replace(/"/g, '&quot;')
  .replace(/'/g, '&apos;')

const isPosition = (p) => Array.isArray(p) && p.length >= 2 && Number.isFinite(Number(p[0])) && Number.isFinite(Number(p[1]))

const fmtCoord = (n) => String(Math.round(Number(n) * 1e7) / 1e7)

const textEl = (indent, tag, value) => (value === undefined || value === null || String(value).trim() === ''
  ? ''
  : `${indent}<${tag}>${escapeXml(value)}</${tag}>\n`)

const pointXml = (tag, position, { time, props = {} } = {}, indent = '  ') => {
  const inner = `${indent}  `
  let body = ''
  const ele = position.length > 2 ? Number(position[2]) : Number(props.ele)
  if (Number.isFinite(ele)) body += `${inner}<ele>${fmtCoord(ele)}</ele>\n`
  body += textEl(inner, 'time', time)
  // Order follows the GPX 1.1 schema sequence (ele, time, …, name, cmt, desc, src, …, sym, type).
  body += textEl(inner, 'name', props.name)
  body += textEl(inner, 'cmt', props.cmt)
  body += textEl(inner, 'desc', props.desc)
  body += textEl(inner, 'src', props.src)
  body += textEl(inner, 'sym', props.sym)
  body += textEl(inner, 'type', props.type)
  const open = `${indent}<${tag} lat="${fmtCoord(position[1])}" lon="${fmtCoord(position[0])}"`
  return body ? `${open}>\n${body}${indent}</${tag}>\n` : `${open}/>\n`
}

const toTimeString = (t) => {
  if (typeof t === 'string' && t.trim()) return t.trim()
  if (typeof t === 'number' && Number.isFinite(t)) return new Date(t).toISOString()
  return null
}

/** Per-position time lookup from properties.coordTimes (flat, or one array per line for MultiLineString). */
const timesFor = (props, lineIndex = 0) => {
  const times = Array.isArray(props?.coordTimes) ? props.coordTimes : null
  return (i) => {
    if (!times) return null
    return toTimeString(Array.isArray(times[0]) ? times[lineIndex]?.[i] : times[i])
  }
}

const segmentXml = (coords, timeAt, tag, indent) => coords
  .map((p, i) => (isPosition(p) ? pointXml(tag, p, { time: timeAt(i) }, indent) : ''))
  .join('')

const hasLine = (coords) => Array.isArray(coords) && coords.filter(isPosition).length >= 2

const trackXml = (lines, props) => {
  let xml = '  <trk>\n'
  xml += textEl('    ', 'name', props.name)
  xml += textEl('    ', 'cmt', props.cmt)
  xml += textEl('    ', 'desc', props.desc)
  xml += textEl('    ', 'src', props.src)
  xml += textEl('    ', 'type', props.type)
  lines.forEach((coords, idx) => {
    if (!hasLine(coords)) return
    xml += '    <trkseg>\n'
    xml += segmentXml(coords, timesFor(props, idx), 'trkpt', '      ')
    xml += '    </trkseg>\n'
  })
  return `${xml}  </trk>\n`
}

const routeXml = (coords, props) => {
  let xml = '  <rte>\n'
  xml += textEl('    ', 'name', props.name)
  xml += textEl('    ', 'cmt', props.cmt)
  xml += textEl('    ', 'desc', props.desc)
  xml += textEl('    ', 'src', props.src)
  xml += textEl('    ', 'type', props.type)
  xml += segmentXml(coords, timesFor(props), 'rtept', '    ')
  return `${xml}  </rte>\n`
}

/**
 * Serialise a FeatureCollection to GPX 1.1.
 * Points/MultiPoints → wpt, LineStrings → trk (or rte when properties.gpxType === 'rte'),
 * MultiLineStrings → one trk with several trkseg. Other geometries are skipped.
 * @returns {{ xml: string, exported: number, skipped: number }}
 */
export const toGpx = (featureCollection, { name, creator = 'Fyr' } = {}) => {
  const features = Array.isArray(featureCollection?.features) ? featureCollection.features : []
  let wpts = ''
  let rtes = ''
  let trks = ''
  let exported = 0
  let skipped = 0

  for (const feature of features) {
    const geom = feature?.geometry
    const props = feature?.properties || {}
    const type = geom?.type
    const coords = geom?.coordinates
    const time = toTimeString(props.time)
    if (type === 'Point' && isPosition(coords)) {
      wpts += pointXml('wpt', coords, { time, props })
      exported++
    } else if (type === 'MultiPoint' && Array.isArray(coords) && coords.some(isPosition)) {
      coords.filter(isPosition).forEach((p) => { wpts += pointXml('wpt', p, { time, props }) })
      exported++
    } else if (type === 'LineString' && hasLine(coords)) {
      if (props.gpxType === 'rte') rtes += routeXml(coords, props)
      else trks += trackXml([coords], props)
      exported++
    } else if (type === 'MultiLineString' && Array.isArray(coords) && coords.some(hasLine)) {
      trks += trackXml(coords, props)
      exported++
    } else {
      skipped++
    }
  }

  let xml = '<?xml version="1.0" encoding="UTF-8"?>\n'
  xml += `<gpx version="1.1" creator="${escapeXml(creator)}" xmlns="${GPX_NS}">\n`
  if (name) xml += `  <metadata>\n    <name>${escapeXml(name)}</name>\n  </metadata>\n`
  xml += wpts + rtes + trks
  xml += '</gpx>\n'
  return { xml, exported, skipped }
}
