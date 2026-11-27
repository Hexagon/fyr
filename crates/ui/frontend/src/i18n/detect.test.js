import { describe, expect, it } from 'vitest'
import {
  DEFAULT_LOCALE,
  LOCALE_STORAGE_KEY,
  detectLocale,
  matchLocale,
  readStoredLocale,
  resolveInitialLocale,
  storeLocale
} from './detect.js'

const memoryStorage = (initial = {}) => {
  const data = { ...initial }
  return {
    data,
    getItem: (k) => (k in data ? data[k] : null),
    setItem: (k, v) => { data[k] = String(v) },
    removeItem: (k) => { delete data[k] }
  }
}

const throwingStorage = {
  getItem: () => { throw new Error('SecurityError') },
  setItem: () => { throw new Error('QuotaExceededError') }
}

describe('matchLocale', () => {
  it('maps regional and underscore tags to the base language', () => {
    expect(matchLocale('sv-SE')).toBe('sv')
    expect(matchLocale('SV_fi')).toBe('sv')
    expect(matchLocale('en-GB')).toBe('en')
    expect(matchLocale(' sv ')).toBe('sv')
  })

  it('returns null for unsupported or empty input', () => {
    expect(matchLocale('de-DE')).toBeNull()
    expect(matchLocale('')).toBeNull()
    expect(matchLocale(null)).toBeNull()
    expect(matchLocale(undefined)).toBeNull()
  })
})

describe('detectLocale', () => {
  it('picks Swedish for any sv* browser language', () => {
    expect(detectLocale(['sv-SE', 'en-US'])).toBe('sv')
    expect(detectLocale(['sv'])).toBe('sv')
    expect(detectLocale(['sv-FI'])).toBe('sv')
  })

  it('uses the first supported language in preference order', () => {
    expect(detectLocale(['de-DE', 'sv-SE', 'en'])).toBe('sv')
    expect(detectLocale(['en-US', 'sv-SE'])).toBe('en')
  })

  it('falls back to English', () => {
    expect(detectLocale(['de-DE', 'fr'])).toBe(DEFAULT_LOCALE)
    expect(detectLocale([])).toBe('en')
    expect(detectLocale(undefined)).toBe('en')
    expect(detectLocale('sv-SE')).toBe('sv')
  })
})

describe('locale persistence', () => {
  it('stores and reads back the choice', () => {
    const storage = memoryStorage()
    expect(storeLocale(storage, 'sv')).toBe(true)
    expect(storage.data[LOCALE_STORAGE_KEY]).toBe('sv')
    expect(readStoredLocale(storage)).toBe('sv')
  })

  it('normalises and rejects unsupported values', () => {
    const storage = memoryStorage()
    expect(storeLocale(storage, 'sv-SE')).toBe(true)
    expect(storage.data[LOCALE_STORAGE_KEY]).toBe('sv')
    expect(storeLocale(storage, 'xx')).toBe(false)
    expect(readStoredLocale(memoryStorage({ [LOCALE_STORAGE_KEY]: 'klingon' }))).toBeNull()
  })

  it('never throws when storage is missing or blocked', () => {
    expect(readStoredLocale(null)).toBeNull()
    expect(storeLocale(null, 'sv')).toBe(false)
    expect(readStoredLocale(throwingStorage)).toBeNull()
    expect(storeLocale(throwingStorage, 'sv')).toBe(false)
  })
})

describe('resolveInitialLocale', () => {
  it('prefers the stored choice over the browser languages', () => {
    const storage = memoryStorage({ [LOCALE_STORAGE_KEY]: 'en' })
    expect(resolveInitialLocale({ storage, navigator: { languages: ['sv-SE'] } })).toBe('en')
  })

  it('detects from navigator.languages, then navigator.language', () => {
    expect(resolveInitialLocale({ storage: memoryStorage(), navigator: { languages: ['sv-SE'] } })).toBe('sv')
    expect(resolveInitialLocale({ storage: null, navigator: { languages: [], language: 'sv' } })).toBe('sv')
    expect(resolveInitialLocale({ storage: throwingStorage, navigator: { language: 'nb-NO' } })).toBe('en')
    expect(resolveInitialLocale()).toBe('en')
  })
})
