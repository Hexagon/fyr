// Translated labels for place kinds from the server's place index
// (`place_kind` on `kind: "place"` search hits: city, village, lake, peak…).
// Kinds come from map data, so unknown ones fall back to a readable form of
// the raw value. Labels live in the `core` catalogue because both the Maps
// and the Search page show them.

import { t } from '../../i18n/index.js'

const KINDS = [
  { kind: 'continent', labelKey: 'placeKind.continent', icon: '🌍' },
  { kind: 'country', labelKey: 'placeKind.country', icon: '🏳️' },
  { kind: 'state', labelKey: 'placeKind.state', icon: '🗺️' },
  { kind: 'province', labelKey: 'placeKind.province', icon: '🗺️' },
  { kind: 'region', labelKey: 'placeKind.region', icon: '🗺️' },
  { kind: 'county', labelKey: 'placeKind.county', icon: '🗺️' },
  { kind: 'municipality', labelKey: 'placeKind.municipality', icon: '🏛️' },
  { kind: 'city', labelKey: 'placeKind.city', icon: '🏙️' },
  { kind: 'town', labelKey: 'placeKind.town', icon: '🏘️' },
  { kind: 'village', labelKey: 'placeKind.village', icon: '🏡' },
  { kind: 'hamlet', labelKey: 'placeKind.hamlet', icon: '🏡' },
  { kind: 'suburb', labelKey: 'placeKind.suburb', icon: '🏘️' },
  { kind: 'borough', labelKey: 'placeKind.borough', icon: '🏘️' },
  { kind: 'quarter', labelKey: 'placeKind.quarter', icon: '🏘️' },
  { kind: 'neighbourhood', labelKey: 'placeKind.neighbourhood', icon: '🏘️' },
  { kind: 'neighborhood', labelKey: 'placeKind.neighbourhood', icon: '🏘️' },
  { kind: 'locality', labelKey: 'placeKind.locality', icon: '📍' },
  { kind: 'isolated_dwelling', labelKey: 'placeKind.isolatedDwelling', icon: '🏠' },
  { kind: 'farm', labelKey: 'placeKind.farm', icon: '🏠' },
  { kind: 'island', labelKey: 'placeKind.island', icon: '🏝️' },
  { kind: 'islet', labelKey: 'placeKind.islet', icon: '🏝️' },
  { kind: 'archipelago', labelKey: 'placeKind.archipelago', icon: '🏝️' },
  { kind: 'ocean', labelKey: 'placeKind.ocean', icon: '🌊' },
  { kind: 'sea', labelKey: 'placeKind.sea', icon: '🌊' },
  { kind: 'bay', labelKey: 'placeKind.bay', icon: '🌊' },
  { kind: 'strait', labelKey: 'placeKind.strait', icon: '🌊' },
  { kind: 'fjord', labelKey: 'placeKind.fjord', icon: '🌊' },
  { kind: 'lake', labelKey: 'placeKind.lake', icon: '💧' },
  { kind: 'lagoon', labelKey: 'placeKind.lagoon', icon: '💧' },
  { kind: 'reservoir', labelKey: 'placeKind.reservoir', icon: '💧' },
  { kind: 'pond', labelKey: 'placeKind.pond', icon: '💧' },
  { kind: 'river', labelKey: 'placeKind.river', icon: '〰️' },
  { kind: 'stream', labelKey: 'placeKind.stream', icon: '〰️' },
  { kind: 'canal', labelKey: 'placeKind.canal', icon: '〰️' },
  { kind: 'water', labelKey: 'placeKind.water', icon: '💧' },
  { kind: 'peak', labelKey: 'placeKind.peak', icon: '⛰️' },
  { kind: 'volcano', labelKey: 'placeKind.volcano', icon: '🌋' },
  { kind: 'hill', labelKey: 'placeKind.hill', icon: '⛰️' },
  { kind: 'saddle', labelKey: 'placeKind.saddle', icon: '⛰️' },
  { kind: 'ridge', labelKey: 'placeKind.ridge', icon: '⛰️' },
  { kind: 'mountain_range', labelKey: 'placeKind.mountainRange', icon: '🏔️' },
  { kind: 'glacier', labelKey: 'placeKind.glacier', icon: '🧊' },
  { kind: 'cape', labelKey: 'placeKind.cape', icon: '🪨' },
  { kind: 'park', labelKey: 'placeKind.park', icon: '🌳' },
  { kind: 'national_park', labelKey: 'placeKind.nationalPark', icon: '🌲' },
  { kind: 'nature_reserve', labelKey: 'placeKind.natureReserve', icon: '🌲' },
  { kind: 'protected_area', labelKey: 'placeKind.protectedArea', icon: '🌲' },
  { kind: 'aerodrome', labelKey: 'placeKind.airport', icon: '✈️' },
  { kind: 'airport', labelKey: 'placeKind.airport', icon: '✈️' },
  { kind: 'place', labelKey: 'placeKind.place', icon: '📍' },
  { kind: 'poi', labelKey: 'placeKind.poi', icon: '📍' }
]

const BY_KIND = new Map(KINDS.map((entry) => [entry.kind, entry]))

/** "mountain_range" → "Mountain range" (fallback for kinds we do not know). */
export function humanizeKind(kind) {
  const text = String(kind || '').replace(/[_-]+/g, ' ').replace(/\s+/g, ' ').trim()
  return text ? text.charAt(0).toUpperCase() + text.slice(1) : ''
}

/** Translated label for a place kind ('' for none). */
export function placeKindLabel(kind) {
  if (!kind) return ''
  const entry = BY_KIND.get(String(kind))
  return entry ? t(entry.labelKey) : humanizeKind(kind)
}

/** Icon for a place kind. */
export function placeKindIcon(kind) {
  return BY_KIND.get(String(kind || ''))?.icon || '📍'
}
