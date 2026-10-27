// Fyr UI internationalisation (vue-i18n, composition API, legacy: false).
//
// English is bundled eagerly and is the fallback for any missing key; other
// locales are separate chunks loaded on demand, so an English-only browser
// never downloads them. Plain JS modules translate through the exported
// `t` / `n` / `locale` helpers; components use `useI18n()` or `$t`.
// See DEVELOPER_MANUAL "Internationalisation" for the conventions.

import { createI18n } from 'vue-i18n'
import en from './locales/en.json'
import { DEFAULT_LOCALE, SUPPORTED_LOCALES, matchLocale, resolveInitialLocale, storeLocale } from './detect.js'
import { getDefaultStorage } from '../modules/library/safeStorage.js'

export { SUPPORTED_LOCALES, DEFAULT_LOCALE }

// Language names are shown in their own language so anyone can find theirs.
export const LOCALE_NAMES = { en: 'English', sv: 'Svenska' }

const loaders = {
  sv: () => import('./locales/sv.json')
}

export const i18n = createI18n({
  legacy: false,
  globalInjection: true,
  locale: DEFAULT_LOCALE,
  fallbackLocale: DEFAULT_LOCALE,
  messages: { en },
  missingWarn: false,
  fallbackWarn: false
})

const global = i18n.global

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

const applyDocumentLang = (code) => {
  try {
    if (typeof document !== 'undefined' && document.documentElement) {
      document.documentElement.setAttribute('lang', code)
    }
  } catch {
    // ignore
  }
}

const loadMessages = async (code) => {
  if (global.availableLocales.includes(code)) return
  const loader = loaders[code]
  if (!loader) return
  const mod = await loader()
  global.setLocaleMessage(code, mod.default || mod)
}

/**
 * Switch the UI language. Loads the locale's messages first so the UI never
 * shows a half-translated state; `persist` stores the choice per browser.
 */
export const setLocale = async (code, { persist = true, storage = getDefaultStorage() } = {}) => {
  const next = matchLocale(code) || DEFAULT_LOCALE
  try {
    await loadMessages(next)
  } catch {
    // The chunk could not be loaded (server gone, flaky link): keep the
    // current language rather than showing raw keys.
    return global.locale.value
  }
  global.locale.value = next
  applyDocumentLang(next)
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
