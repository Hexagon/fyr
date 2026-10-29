import { describe, expect, it } from 'vitest'
import { READER_PREFS_KEY, clampFontScale, loadReaderPrefs, normalizeReaderPrefs, saveReaderPrefs } from './readerPrefs'

const memory = () => {
  const data = new Map()
  return {
    getItem: (k) => (data.has(k) ? data.get(k) : null),
    setItem: (k, v) => data.set(k, String(v)),
    removeItem: (k) => data.delete(k)
  }
}

describe('reader prefs', () => {
  it('defaults when nothing or garbage is stored', () => {
    expect(loadReaderPrefs(memory())).toEqual({ theme: 'light', fontScale: 100 })
    const storage = memory()
    storage.setItem(READER_PREFS_KEY, '{oops')
    expect(loadReaderPrefs(storage)).toEqual({ theme: 'light', fontScale: 100 })
    expect(normalizeReaderPrefs({ theme: 'neon', fontScale: 'big' })).toEqual({ theme: 'light', fontScale: 100 })
  })

  it('clamps and steps the font scale', () => {
    expect(clampFontScale(10)).toBe(70)
    expect(clampFontScale(1000)).toBe(200)
    expect(clampFontScale(114)).toBe(110)
  })

  it('persists and reloads', () => {
    const storage = memory()
    expect(saveReaderPrefs({ theme: 'sepia', fontScale: 130 }, storage)).toEqual({ theme: 'sepia', fontScale: 130 })
    expect(loadReaderPrefs(storage)).toEqual({ theme: 'sepia', fontScale: 130 })
  })

  it('never throws when storage throws', () => {
    const throwing = {
      getItem: () => {
        throw new Error('blocked')
      },
      setItem: () => {
        throw new Error('blocked')
      }
    }
    expect(loadReaderPrefs(throwing)).toEqual({ theme: 'light', fontScale: 100 })
    expect(saveReaderPrefs({ theme: 'dark' }, throwing)).toEqual({ theme: 'dark', fontScale: 100 })
  })
})
