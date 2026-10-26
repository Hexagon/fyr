// Per-browser reader display preferences (theme + font scale) for EPUB and
// Markdown. Stored in localStorage via safeStorage; never throws.

import { getDefaultStorage, readJson, writeJson } from './safeStorage'

export const READER_PREFS_KEY = 'fyr.reader.prefs.v1'
export const READER_THEMES = ['light', 'sepia', 'dark']
export const FONT_SCALE_MIN = 70
export const FONT_SCALE_MAX = 200
export const FONT_SCALE_STEP = 10

export const DEFAULT_READER_PREFS = Object.freeze({ theme: 'light', fontScale: 100 })

/** Theme palettes shared by the EPUB iframe theme and the Markdown pane. */
export const READER_THEME_COLORS = Object.freeze({
  light: { background: '#ffffff', text: '#111111', link: '#1a5fb4', code: '#eef1f4', border: '#dbe1e6' },
  sepia: { background: '#f4ecd8', text: '#433422', link: '#7a4b12', code: '#eadfc4', border: '#d8c9a8' },
  dark: { background: '#16191d', text: '#d9dee3', link: '#7ab7ff', code: '#22272d', border: '#343b43' }
})

export const clampFontScale = (value) => {
  const n = Number(value)
  if (!Number.isFinite(n)) return DEFAULT_READER_PREFS.fontScale
  const stepped = Math.round(n / FONT_SCALE_STEP) * FONT_SCALE_STEP
  return Math.min(FONT_SCALE_MAX, Math.max(FONT_SCALE_MIN, stepped))
}

export const normalizeReaderPrefs = (raw) => {
  const prefs = { ...DEFAULT_READER_PREFS }
  if (!raw || typeof raw !== 'object') return prefs
  if (READER_THEMES.includes(raw.theme)) prefs.theme = raw.theme
  if (raw.fontScale !== undefined) prefs.fontScale = clampFontScale(raw.fontScale)
  return prefs
}

export const loadReaderPrefs = (storage = getDefaultStorage()) =>
  normalizeReaderPrefs(readJson(storage, READER_PREFS_KEY, null))

export const saveReaderPrefs = (prefs, storage = getDefaultStorage()) => {
  const normalized = normalizeReaderPrefs(prefs)
  writeJson(storage, READER_PREFS_KEY, normalized)
  return normalized
}
