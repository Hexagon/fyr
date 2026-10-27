// Guards for the translation catalogues: every locale has the same keys and
// placeholders as English, every message compiles, and every literal key
// used in the source exists. Run with `npm test`.
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { createI18n } from 'vue-i18n'
import en from './locales/en.json'
import sv from './locales/sv.json'
import { SUPPORTED_LOCALES } from './detect.js'

const LOCALES = { en, sv }
const SRC = join(dirname(fileURLToPath(import.meta.url)), '..')

const flatten = (tree, prefix = '', out = {}) => {
  for (const [key, value] of Object.entries(tree)) {
    const path = prefix ? `${prefix}.${key}` : key
    if (value && typeof value === 'object') flatten(value, path, out)
    else out[path] = value
  }
  return out
}

const placeholders = (message) => [...new Set(
  [...String(message).matchAll(/\{\s*([A-Za-z_]\w*)\s*\}/g)].map((m) => m[1])
)].sort()

const flat = Object.fromEntries(Object.entries(LOCALES).map(([code, tree]) => [code, flatten(tree)]))

const sourceFiles = (dir, out = []) => {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) sourceFiles(path, out)
    else if (/\.(vue|js)$/.test(name) && !/\.test\.js$/.test(name)) out.push(path)
  }
  return out
}

// t('key'), $t('key'), te('key'), keypath="key" and `somethingKey: 'key'`
// (route meta / option tables that are translated later).
const KEY_PATTERNS = [
  /(?<![\w.])\$?te?\(\s*['"`]([A-Za-z][\w-]*(?:\.[\w-]+)+)['"`]/g,
  /\bkeypath="([A-Za-z][\w-]*(?:\.[\w-]+)+)"/g,
  /\b\w*Key:\s*['"]([A-Za-z][\w-]*(?:\.[\w-]+)+)['"]/g
]

const usedKeys = () => {
  const keys = new Map()
  for (const file of sourceFiles(SRC)) {
    const text = readFileSync(file, 'utf8')
    for (const pattern of KEY_PATTERNS) {
      for (const match of text.matchAll(pattern)) {
        if (!keys.has(match[1])) keys.set(match[1], relative(SRC, file))
      }
    }
  }
  return keys
}

describe('locale catalogues', () => {
  afterEach(() => vi.restoreAllMocks())

  it('ships a catalogue for every supported locale', () => {
    expect(Object.keys(LOCALES).sort()).toEqual([...SUPPORTED_LOCALES].sort())
  })

  it('has more than a handful of keys', () => {
    expect(Object.keys(flat.en).length).toBeGreaterThan(100)
  })

  for (const code of Object.keys(LOCALES).filter((c) => c !== 'en')) {
    it(`${code} has exactly the same keys as en`, () => {
      const enKeys = Object.keys(flat.en).sort()
      const keys = Object.keys(flat[code]).sort()
      expect(keys.filter((k) => !(k in flat.en)), `keys only in ${code}`).toEqual([])
      expect(enKeys.filter((k) => !(k in flat[code])), `keys missing from ${code}`).toEqual([])
    })

    it(`${code} uses the same {placeholders} as en`, () => {
      const mismatched = Object.keys(flat.en)
        .filter((k) => k in flat[code])
        .filter((k) => placeholders(flat.en[k]).join() !== placeholders(flat[code][k]).join())
      expect(mismatched).toEqual([])
    })
  }

  it('has no empty messages', () => {
    for (const [code, messages] of Object.entries(flat)) {
      const empty = Object.entries(messages).filter(([, v]) => typeof v !== 'string' || !v.trim()).map(([k]) => k)
      expect(empty, code).toEqual([])
    }
  })

  it('compiles and renders every message without warnings', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const error = vi.spyOn(console, 'error').mockImplementation(() => {})
    const i18n = createI18n({ legacy: false, locale: 'en', messages: LOCALES, missingWarn: false, fallbackWarn: false })
    const broken = []
    for (const [code, messages] of Object.entries(flat)) {
      i18n.global.locale.value = code
      for (const [key, message] of Object.entries(messages)) {
        const params = Object.fromEntries(placeholders(message).map((p) => [p, `<${p}>`]))
        try {
          const out = i18n.global.t(key, params, 2)
          if (typeof out !== 'string' || out === key || /\{\s*\w+\s*\}/.test(out)) broken.push(`${code}:${key}`)
        } catch (err) {
          broken.push(`${code}:${key} ${err.message}`)
        }
      }
    }
    expect(broken).toEqual([])
    expect(warn).not.toHaveBeenCalled()
    expect(error).not.toHaveBeenCalled()
  })

  it('defines every literal key used in the source', () => {
    const missing = [...usedKeys()].filter(([key]) => !(key in flat.en)).map(([key, file]) => `${key} (${file})`)
    expect(missing).toEqual([])
  })
})
