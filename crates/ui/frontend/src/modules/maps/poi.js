// GeoJSON helpers for user POI files (served from /data/poi, saved via PUT /api/poi/:filename).

export const emptyGeoJSON = () => ({ type: 'FeatureCollection', features: [] })

/** Coerce whatever /data/poi returned into a FeatureCollection with a features array. */
export const normalizePoiData = (data) => {
  if (data && Array.isArray(data.features)) {
    return data.type === 'FeatureCollection' ? data : { ...data, type: 'FeatureCollection' }
  }
  if (data && data.type === 'Feature' && data.geometry) {
    return { type: 'FeatureCollection', features: [data] }
  }
  return emptyGeoJSON()
}

/** Append `.geojson` unless the name already ends with .geojson/.json. */
export const normalizePoiFilename = (raw) => {
  const name = String(raw || '').trim()
  if (!name) return ''
  const lower = name.toLowerCase()
  return (lower.endsWith('.geojson') || lower.endsWith('.json')) ? name : `${name}.geojson`
}

/** Filesystem-friendly base name derived from an arbitrary label or file name. */
export const slugifyFilename = (raw, fallback = 'import') => {
  const base = String(raw || '')
    .replace(/\.[^./\\]+$/, '')
    .normalize('NFKD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/[^A-Za-z0-9._-]+/g, '-')
    .replace(/^[-.]+|[-.]+$/g, '')
    .slice(0, 80)
  return base || fallback
}

/** Return `<base>.geojson`, adding -2, -3… until it does not collide with `existing`. */
export const uniquePoiFilename = (base, existing = []) => {
  const taken = new Set((existing || []).map((n) => String(n).toLowerCase()))
  const stem = slugifyFilename(base)
  let candidate = `${stem}.geojson`
  for (let i = 2; taken.has(candidate.toLowerCase()); i++) {
    candidate = `${stem}-${i}.geojson`
  }
  return candidate
}

export const makePointFeature = (coords, name, extraProps = {}) => ({
  type: 'Feature',
  geometry: { type: 'Point', coordinates: coords },
  properties: { name, ...extraProps }
})

/** GeoJSON used by the measure tool: a dashed line plus vertex points. */
export const buildMeasureGeoJSON = (points) => {
  const features = []
  if (points.length > 1) {
    features.push({
      type: 'Feature',
      geometry: { type: 'LineString', coordinates: points },
      properties: {}
    })
  }
  for (const pt of points) {
    features.push({ type: 'Feature', geometry: { type: 'Point', coordinates: pt }, properties: {} })
  }
  return { type: 'FeatureCollection', features }
}
