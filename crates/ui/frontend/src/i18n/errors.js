// Errors that carry a translation key.
//
// Plain modules (crypto, GPX parsing, file helpers) stay free of vue-i18n:
// they throw an Error with readable English text (for logs and tests) plus an
// `i18nKey`/`i18nParams` pair the UI translates with `errorText()` from
// ./index.js.

export const localizedError = (key, message, params = undefined) => {
  const error = new Error(message)
  error.i18nKey = key
  if (params) error.i18nParams = params
  return error
}
