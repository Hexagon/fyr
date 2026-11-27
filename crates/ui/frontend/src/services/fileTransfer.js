// Small helpers for client-side file export/import (no server involved).
import { localizedError } from '../i18n/errors.js'

/** Offer `blob` to the user as a download named `filename`. */
export const saveBlob = (blob, filename) => {
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = filename
  link.rel = 'noopener'
  document.body.appendChild(link)
  link.click()
  link.remove()
  setTimeout(() => URL.revokeObjectURL(url), 10_000)
}

export const saveText = (text, filename, type = 'application/json') =>
  saveBlob(new Blob([text], { type }), filename)

/** Read a user-picked file as text, refusing anything over `maxBytes`. */
export const readFileText = async (file, maxBytes = 5 * 1024 * 1024) => {
  if (!file) throw localizedError('files.noFile', 'No file selected')
  if (file.size > maxBytes) throw localizedError('files.tooLarge', `File is too large (max ${Math.round(maxBytes / 1024)} KB)`, { kb: Math.round(maxBytes / 1024) })
  return file.text()
}

/** `fyr-<kind>-YYYY-MM-DD.json` */
export const datedFilename = (kind, ext = 'json', date = new Date()) =>
  `fyr-${kind}-${date.toISOString().slice(0, 10)}.${ext}`
