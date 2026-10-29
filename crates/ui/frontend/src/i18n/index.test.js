import { afterEach, describe, expect, it } from 'vitest'
import { LOCALE_STORAGE_KEY } from './detect.js'
import { formatNumber, i18n, locale, setLocale, t } from './index.js'

const memoryStorage = () => {
  const data = {}
  return {
    data,
    getItem: (k) => (k in data ? data[k] : null),
    setItem: (k, v) => { data[k] = String(v) }
  }
}

describe('i18n runtime', () => {
  afterEach(async () => {
    await setLocale('en', { persist: false })
  })

  it('starts in English', () => {
    expect(locale()).toBe('en')
    expect(t('nav.books')).toBe('Library')
  })

  it('loads Swedish on demand, updates <html lang> and persists the choice', async () => {
    const storage = memoryStorage()
    await setLocale('sv', { storage })
    expect(locale()).toBe('sv')
    expect(t('nav.books')).toBe('Bibliotek')
    expect(document.documentElement.getAttribute('lang')).toBe('sv')
    expect(storage.data[LOCALE_STORAGE_KEY]).toBe('sv')
    expect(i18n.global.availableLocales).toContain('sv')
  })

  it('does not persist when asked not to and maps unknown codes to English', async () => {
    const storage = memoryStorage()
    await setLocale('sv', { storage, persist: false })
    expect(storage.data[LOCALE_STORAGE_KEY]).toBeUndefined()
    await setLocale('xx', { storage })
    expect(locale()).toBe('en')
    expect(document.documentElement.getAttribute('lang')).toBe('en')
  })

  it('formats numbers for the active locale', async () => {
    expect(formatNumber(1234.5)).toBe('1,234.5')
    await setLocale('sv', { persist: false })
    expect(formatNumber(1.5)).toBe('1,5')
  })
})
