import { describe, expect, it } from 'vitest'
import { parseGpx, toGpx, escapeXml } from './gpx.js'

const GPX11 = `<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="Test" xmlns="http://www.topografix.com/GPX/1/1"
     xmlns:gpxtpx="http://www.garmin.com/xmlschemas/TrackPointExtension/v1">
  <metadata><name>Kungsleden day 1</name></metadata>
  <wpt lat="68.3497" lon="18.8306">
    <ele>385.5</ele>
    <time>2026-07-01T08:00:00Z</time>
    <name>Abisko &amp; turiststation</name>
    <desc>Start</desc>
    <sym>Lodge</sym>
  </wpt>
  <wpt lat="68.2" lon="18.7"></wpt>
  <trk>
    <name>Day 1</name>
    <trkseg>
      <trkpt lat="68.3497" lon="18.8306"><ele>385</ele><time>2026-07-01T08:00:00Z</time>
        <extensions><gpxtpx:TrackPointExtension><gpxtpx:hr>120</gpxtpx:hr></gpxtpx:TrackPointExtension></extensions>
      </trkpt>
      <trkpt lat="68.34" lon="18.82"><ele>400</ele><time>2026-07-01T08:10:00Z</time></trkpt>
    </trkseg>
    <trkseg>
      <trkpt lat="68.30" lon="18.80"><time>2026-07-01T09:00:00Z</time></trkpt>
      <trkpt lat="68.29" lon="18.79"></trkpt>
    </trkseg>
  </trk>
  <rte>
    <rtept lat="68.1" lon="18.6"/>
    <rtept lat="68.0" lon="18.5"/>
  </rte>
</gpx>`

describe('parseGpx', () => {
  it('parses waypoints with elevation, time and descriptive properties', () => {
    const { name, features, counts } = parseGpx(GPX11)
    expect(name).toBe('Kungsleden day 1')
    expect(counts).toEqual({ waypoints: 2, tracks: 1, routes: 1, skippedPoints: 0 })
    const wpt = features[0]
    expect(wpt.geometry).toEqual({ type: 'Point', coordinates: [18.8306, 68.3497, 385.5] })
    expect(wpt.properties).toMatchObject({
      name: 'Abisko & turiststation',
      desc: 'Start',
      sym: 'Lodge',
      time: '2026-07-01T08:00:00Z',
      gpxType: 'wpt'
    })
  })

  it('names unnamed waypoints, tracks and routes', () => {
    const { features } = parseGpx(GPX11)
    expect(features[1].properties.name).toBe('Waypoint 2')
    expect(features[1].geometry.coordinates).toEqual([18.7, 68.2])
    expect(features.find((f) => f.properties.gpxType === 'rte').properties.name).toBe('Route 1')
    const unnamedTrack = parseGpx('<gpx><trk><trkseg><trkpt lat="1" lon="2"/><trkpt lat="1.1" lon="2.1"/></trkseg></trk></gpx>')
    expect(unnamedTrack.features[0].properties.name).toBe('Track 1')
  })

  it('splits multi-segment tracks into LineStrings with coordTimes', () => {
    const lines = parseGpx(GPX11).features.filter((f) => f.properties.gpxType === 'trk')
    expect(lines).toHaveLength(2)
    expect(lines.map((l) => l.properties.name)).toEqual(['Day 1 (1/2)', 'Day 1 (2/2)'])
    expect(lines.map((l) => l.properties.segment)).toEqual([1, 2])
    expect(lines[0].geometry).toEqual({ type: 'LineString', coordinates: [[18.8306, 68.3497, 385], [18.82, 68.34, 400]] })
    expect(lines[0].properties.coordTimes).toEqual(['2026-07-01T08:00:00Z', '2026-07-01T08:10:00Z'])
    // Missing times are kept positionally as null; missing elevation leaves a 2D position.
    expect(lines[1].properties.coordTimes).toEqual(['2026-07-01T09:00:00Z', null])
    expect(lines[1].geometry.coordinates[1]).toEqual([18.79, 68.29])
  })

  it('parses routes as LineStrings without coordTimes when no times exist', () => {
    const rte = parseGpx(GPX11).features.find((f) => f.properties.gpxType === 'rte')
    expect(rte.geometry.coordinates).toEqual([[18.6, 68.1], [18.5, 68.0]])
    expect(rte.properties.coordTimes).toBeUndefined()
  })

  it('handles GPX 1.0 and prefixed namespaces', () => {
    const v10 = parseGpx('<gpx version="1.0" xmlns="http://www.topografix.com/GPX/1/0"><name>Old</name><wpt lat="1" lon="2"><name>A</name></wpt></gpx>')
    expect(v10.name).toBe('Old')
    expect(v10.features[0].properties.name).toBe('A')
    const prefixed = parseGpx('<g:gpx xmlns:g="http://www.topografix.com/GPX/1/1"><g:wpt lat="1" lon="2"><g:name>P</g:name></g:wpt></g:gpx>')
    expect(prefixed.features[0].properties.name).toBe('P')
  })

  it('skips invalid points and too-short lines', () => {
    const { features, counts } = parseGpx(`<gpx>
      <wpt lat="95" lon="0"/><wpt lat="abc" lon="1"/><wpt lon="1"/>
      <trk><trkseg><trkpt lat="1" lon="2"/></trkseg></trk>
      <rte><rtept lat="1" lon="2"/><rtept lat="x" lon="2"/></rte>
    </gpx>`)
    expect(features).toHaveLength(0)
    expect(counts.skippedPoints).toBe(4)
    expect(counts.tracks).toBe(0)
    expect(counts.routes).toBe(0)
  })

  it('rejects malformed XML and non-GPX documents', () => {
    expect(() => parseGpx('<gpx><wpt')).toThrow(/valid XML/)
    expect(() => parseGpx('<kml></kml>')).toThrow(/not a GPX/)
    expect(() => parseGpx('')).toThrow()
  })
})

describe('escapeXml', () => {
  it('escapes markup characters and strips invalid control characters', () => {
    expect(escapeXml(`<a href="x">Tom & Jerry's</a>`)).toBe('&lt;a href=&quot;x&quot;&gt;Tom &amp; Jerry&apos;s&lt;/a&gt;')
    expect(escapeXml('bad\u0000\u0007char\ttab')).toBe('badchar\ttab')
    expect(escapeXml(null)).toBe('')
    expect(escapeXml('emoji 🏕️')).toBe('emoji 🏕️')
  })
})

describe('toGpx', () => {
  const fc = {
    type: 'FeatureCollection',
    features: [
      { type: 'Feature', geometry: { type: 'Point', coordinates: [18.07, 59.33, 12] }, properties: { name: 'Camp <1> & "fire"', desc: "It's", time: '2026-07-01T08:00:00Z', sym: 'Campground' } },
      { type: 'Feature', geometry: { type: 'Point', coordinates: [-70.5, -33.4] }, properties: {} },
      {
        type: 'Feature',
        geometry: { type: 'LineString', coordinates: [[18, 59, 10], [18.1, 59.1, 20]] },
        properties: { name: 'Walk', coordTimes: ['2026-07-01T08:00:00Z', '2026-07-01T08:30:00Z'] }
      },
      { type: 'Feature', geometry: { type: 'LineString', coordinates: [[1, 2], [3, 4]] }, properties: { name: 'Plan', gpxType: 'rte' } },
      { type: 'Feature', geometry: { type: 'MultiLineString', coordinates: [[[1, 1], [2, 2]], [[3, 3], [4, 4]]] }, properties: { name: 'Multi' } },
      { type: 'Feature', geometry: { type: 'Polygon', coordinates: [[[0, 0], [1, 0], [1, 1], [0, 0]]] }, properties: { name: 'Area' } },
      { type: 'Feature', geometry: null, properties: {} }
    ]
  }

  it('produces well-formed GPX 1.1 and reports skipped geometries', () => {
    const { xml, exported, skipped } = toGpx(fc, { name: 'My <POIs>' })
    expect(exported).toBe(5)
    expect(skipped).toBe(2)
    expect(xml.startsWith('<?xml version="1.0" encoding="UTF-8"?>')).toBe(true)
    expect(xml).toContain('xmlns="http://www.topografix.com/GPX/1/1"')
    expect(xml).toContain('<name>My &lt;POIs&gt;</name>')
    const doc = new DOMParser().parseFromString(xml, 'application/xml')
    expect(doc.getElementsByTagName('parsererror').length).toBe(0)
  })

  it('escapes names and keeps elevation/time', () => {
    const { xml } = toGpx(fc)
    expect(xml).toContain('<name>Camp &lt;1&gt; &amp; &quot;fire&quot;</name>')
    expect(xml).toContain('<desc>It&apos;s</desc>')
    expect(xml).toContain('<wpt lat="59.33" lon="18.07">')
    expect(xml).toContain('<ele>12</ele>')
    expect(xml).toContain('<time>2026-07-01T08:30:00Z</time>')
    expect(xml).toContain('<wpt lat="-33.4" lon="-70.5"/>')
    expect(xml).toContain('<rte>')
    expect((xml.match(/<trkseg>/g) || []).length).toBe(3)
  })

  it('round-trips through parseGpx', () => {
    const { xml } = toGpx(fc)
    const { features } = parseGpx(xml)
    const camp = features.find((f) => f.properties.name === 'Camp <1> & "fire"')
    expect(camp.geometry.coordinates).toEqual([18.07, 59.33, 12])
    expect(camp.properties).toMatchObject({ desc: "It's", sym: 'Campground', time: '2026-07-01T08:00:00Z' })
    const walk = features.find((f) => f.properties.name === 'Walk')
    expect(walk.geometry.coordinates).toEqual([[18, 59, 10], [18.1, 59.1, 20]])
    expect(walk.properties.coordTimes).toEqual(['2026-07-01T08:00:00Z', '2026-07-01T08:30:00Z'])
    expect(features.find((f) => f.properties.name === 'Plan').properties.gpxType).toBe('rte')
    expect(features.filter((f) => f.properties.name?.startsWith('Multi'))).toHaveLength(2)
  })

  it('handles empty input', () => {
    const { xml, exported, skipped } = toGpx(null)
    expect(exported).toBe(0)
    expect(skipped).toBe(0)
    expect(parseGpx(xml).features).toEqual([])
  })

  it('converts numeric times and ignores invalid positions without misaligning times', () => {
    const { xml } = toGpx({
      type: 'FeatureCollection',
      features: [{
        type: 'Feature',
        geometry: { type: 'LineString', coordinates: [[0, 0], ['x', 1], [1, 1], [2, 2]] },
        properties: { coordTimes: [Date.UTC(2026, 0, 1), 'bad-pos', '2026-01-01T00:02:00Z', null] }
      }]
    })
    const line = parseGpx(xml).features[0]
    expect(line.geometry.coordinates).toEqual([[0, 0], [1, 1], [2, 2]])
    expect(line.properties.coordTimes).toEqual(['2026-01-01T00:00:00.000Z', '2026-01-01T00:02:00Z', null])
  })
})
