// Fyr UI internationalisation (vue-i18n, composition API, legacy: false).
//
// Messages live in src/i18n/locales/<locale>/<namespace>.json. Each file has
// one top-level key per namespace (core.json holds the shared ones: app,
// nav, routes, common, time, …). Only English `core` is bundled eagerly; it
// is also the fallback. Every other file is its own small chunk, loaded when
// a page that needs it is opened (see `withMessages` in main.js) or when the
// language changes, so a page chunk and its strings arrive together and an
// English-only browser never downloads Swedish.
//
// Plain JS modules translate through the exported `t` / `formatNumber`
// helpers; components use `useI18n()` or `$t`. See DEVELOPER_MANUAL
// "Internationalisation" for the conventions.

import { createI18n } from 'vue-i18n'
import enCore from './locales/en/core.json'
import { DEFAULT_LOCALE, SUPPORTED_LOCALES, matchLocale, resolveInitialLocale, storeLocale } from './detect.js'
import { getDefaultStorage } from '../modules/library/safeStorage.js'

export { SUPPORTED_LOCALES, DEFAULT_LOCALE }

// Language names are shown in their own language so anyone can find theirs.
export const LOCALE_NAMES = { en: 'English', sv: 'Svenska' }

// Lazy loaders keyed by "<locale>/<namespace>".
const loaders = Object.fromEntries(
  Object.entries(import.meta.glob(['./locales/*/*.json', '!./locales/en/core.json'])).map(([path, load]) => [
    path.replace(/^\.\/locales\//, '').replace(/\.json$/, ''),
    load
  ])
)

export const NAMESPACES = [...new Set(Object.keys(loaders).map((key) => key.split('/')[1]))]

export const i18n = createI18n({
  legacy: false,
  globalInjection: true,
  locale: DEFAULT_LOCALE,
  fallbackLocale: DEFAULT_LOCALE,
  messages: { en: enCore },
  missingWarn: false,
  fallbackWarn: false
})

const global = i18n.global

// Namespaces that pages have asked for so far (core is always needed), and
// the "<locale>/<namespace>" pairs already merged into vue-i18n.
const wantedNamespaces = new Set(['core'])
const loaded = new Set(['en/core'])

/** Translate outside components (modules, services). */
export const t = (...args) => global.t(...args)

/** The active locale code ('en', 'sv'); reactive when read inside computed/render. */
export const locale = () => global.locale.value

/**
 * User-facing text for a caught error: the translation of a localizedError()
 * key, else the error's own message (e.g. verbatim server text), else the
 * translated fallback key.
 */
export const errorText = (err, fallbackKey, fallbackParams = {}) => {
  if (err?.i18nKey) return global.t(err.i18nKey, err.i18nParams || {})
  const message = typeof err === 'string' ? err : err?.message
  return message || global.t(fallbackKey, fallbackParams)
}

/** Locale-aware number formatting (Intl.NumberFormat options). */
export const formatNumber = (value, options = {}) => {
  try {
    return new Intl.NumberFormat(locale(), options).format(value)
  } catch {
    return String(value)
  }
}

const BYTE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB']

/** Human-readable byte count with a locale decimal separator ("1.5 GB" / "1,5 GB"). */
export const formatBytes = (bytes) => {
  const value = Number(bytes)
  if (!Number.isFinite(value) || value <= 0) return '0 B'
  const i = Math.min(BYTE_UNITS.length - 1, Math.floor(Math.log(value) / Math.log(1024)))
  const amount = Math.round((value / Math.pow(1024, i)) * 100) / 100
  return `${formatNumber(amount, { maximumFractionDigits: 2, useGrouping: false })} ${BYTE_UNITS[i]}`
}

const applyDocumentLang = (code) => {
  try {
    if (typeof document !== 'undefined' && document.documentElement) {
      document.documentElement.setAttribute('lang', code)
    }
  } catch {
    // ignore
  }
}

const loadPair = async (code, namespace) => {
  const key = `${code}/${namespace}`
  if (loaded.has(key) || !loaders[key]) return
  const mod = await loaders[key]()
  global.mergeLocaleMessage(code, mod.default || mod)
  loaded.add(key)
}

// English is the fallback, so it is loaded alongside any other locale.
const loadNamespacesFor = (code, namespaces) => Promise.all(
  [...new Set([code, DEFAULT_LOCALE])].flatMap((lang) => namespaces.map((ns) => loadPair(lang, ns)))
)

/**
 * Make sure a namespace's messages are available in the active (and
 * fallback) locale. Pages call this through their route loader.
 */
export const loadNamespace = async (namespace) => {
  wantedNamespaces.add(namespace)
  try {
    await loadNamespacesFor(global.locale.value, [namespace])
  } catch {
    // Offline chunk failure: keys fall back to whatever is loaded.
  }
}

/** Resolve `componentPromise` once the component's message namespace is loaded too. */
export const withMessages = (componentPromise, namespace) =>
  Promise.all([componentPromise, loadNamespace(namespace)]).then(([component]) => component)

/**
 * Switch the UI language. Loads the locale's messages (core plus every
 * namespace used so far) first so the UI never shows a half-translated
 * state; `persist` stores the choice per browser.
 */
export const setLocale = async (code, { persist = true, storage = getDefaultStorage() } = {}) => {
  const next = matchLocale(code) || DEFAULT_LOCALE
  try {
    await loadNamespacesFor(next, [...wantedNamespaces])
  } catch {
    // The chunk could not be loaded (server gone, flaky link): keep the
    // current language rather than showing raw keys.
    return global.locale.value
  }
  global.locale.value = next
  applyDocumentLang(next)
  try {
    if (typeof document !== 'undefined') document.title = global.t('app.documentTitle')
  } catch {
    // ignore
  }
  if (persist) storeLocale(storage, next)
  return next
}

/** Resolve the initial language (stored choice, then browser) and apply it. */
export const initLocale = () => setLocale(
  resolveInitialLocale({
    storage: getDefaultStorage(),
    navigator: typeof navigator !== 'undefined' ? navigator : null
  }),
  { persist: false }
)
