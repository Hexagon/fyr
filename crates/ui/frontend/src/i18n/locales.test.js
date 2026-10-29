// Guards for the translation catalogues: every locale has the same keys and
// placeholders as English, every message compiles, and every literal key
// used in the source exists. Run with `npm test`.
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { createI18n } from 'vue-i18n'
import { SUPPORTED_LOCALES } from './detect.js'

// locales/<locale>/<namespace>.json, merged per locale.
const files = import.meta.glob('./locales/*/*.json', { eager: true, import: 'default' })
const LOCALES = {}
const namespacesByLocale = {}
for (const [path, data] of Object.entries(files)) {
  const [, code, namespace] = path.match(/^\.\/locales\/([^/]+)\/([^/]+)\.json$/)
  LOCALES[code] = { ...(LOCALES[code] || {}), ...data }
  ;(namespacesByLocale[code] ||= []).push(namespace)
}
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

  it('splits every locale into the same namespace files', () => {
    for (const code of Object.keys(LOCALES)) {
      expect(namespacesByLocale[code].sort(), code).toEqual(namespacesByLocale.en.sort())
    }
  })

  it('keeps each namespace file to its own top-level key (core excepted)', () => {
    for (const [path, data] of Object.entries(files)) {
      const namespace = path.match(/([^/]+)\.json$/)[1]
      if (namespace !== 'core') expect(Object.keys(data), path).toEqual([namespace])
    }
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

  // Page chunks load only core + their own namespace, so a page (and the
  // shared components on every page) must not use another page's keys.
  it('uses only core keys and its own namespace on each page', () => {
    const coreKeys = new Set(Object.keys(flatten(files['./locales/en/core.json'])).map((k) => k.split('.')[0]))
    const pageNamespace = {
      'pages/Home.vue': null,
      'pages/Maps.vue': 'maps',
      'pages/Books.vue': 'books',
      'pages/ContentManager.vue': 'content',
      'pages/Assistant.vue': 'assistant',
      'pages/Messages.vue': 'messages',
      'pages/Tools.vue': 'tools',
      'pages/Search.vue': 'search',
      'pages/Settings.vue': 'settings',
      'pages/Login.vue': 'login',
      'App.vue': null,
      'components/HeaderSearch.vue': null,
      'components/LanguageSwitcher.vue': null,
      'modules/messaging/AlertBanner.vue': null,
      'modules/messaging/format.js': null,
      'services/api.js': null
    }
    const wrong = []
    for (const [file, namespace] of Object.entries(pageNamespace)) {
      const text = readFileSync(join(SRC, file), 'utf8')
      for (const pattern of KEY_PATTERNS) {
        for (const match of text.matchAll(pattern)) {
          const top = match[1].split('.')[0]
          if (!coreKeys.has(top) && top !== namespace) wrong.push(`${file}: ${match[1]}`)
        }
      }
    }
    expect(wrong).toEqual([])
  })

  it('defines every literal key used in the source', () => {
    const missing = [...usedKeys()].filter(([key]) => !(key in flat.en)).map(([key, file]) => `${key} (${file})`)
    expect(missing).toEqual([])
  })
})
