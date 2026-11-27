// Language detection and per-browser persistence. Pure helpers so they can be
// unit-tested without a DOM; the storage argument is any Web Storage-like
// object (or null when storage is unavailable).
//
// The choice is stored per browser only, never on the server: in password
// mode guests cannot write settings, and every household member may prefer a
// different language.

export const SUPPORTED_LOCALES = ['en', 'sv']
export const DEFAULT_LOCALE = 'en'
export const LOCALE_STORAGE_KEY = 'fyr.locale'

/** Map a BCP 47 tag ("sv-SE", "en_GB") to a supported locale, or null. */
export const matchLocale = (tag) => {
  const base = String(tag ?? '').trim().toLowerCase().split(/[-_]/)[0]
  return SUPPORTED_LOCALES.includes(base) ? base : null
}

/**
 * Pick the UI locale from the browser's preferred languages: the first entry
 * that maps to a supported locale wins; anything else falls back to English.
 */
export const detectLocale = (languages) => {
  const list = Array.isArray(languages) ? languages : (languages ? [languages] : [])
  for (const tag of list) {
    const match = matchLocale(tag)
    if (match) return match
  }
  return DEFAULT_LOCALE
}

export const readStoredLocale = (storage) => {
  if (!storage) return null
  try {
    return matchLocale(storage.getItem(LOCALE_STORAGE_KEY))
  } catch {
    return null
  }
}

/** Returns true when the choice was persisted. */
export const storeLocale = (storage, locale) => {
  const match = matchLocale(locale)
  if (!storage || !match) return false
  try {
    storage.setItem(LOCALE_STORAGE_KEY, match)
    return true
  } catch {
    return false
  }
}

const browserLanguages = (nav) => {
  if (!nav) return []
  try {
    if (Array.isArray(nav.languages) && nav.languages.length) return nav.languages
    if (nav.languages && typeof nav.languages.length === 'number' && nav.languages.length) {
      return Array.from(nav.languages)
    }
    return nav.language ? [nav.language] : []
  } catch {
    return []
  }
}

/** Stored choice first, then the browser's languages, then English. */
export const resolveInitialLocale = ({ storage = null, navigator: nav = null } = {}) =>
  readStoredLocale(storage) || detectLocale(browserLanguages(nav))
