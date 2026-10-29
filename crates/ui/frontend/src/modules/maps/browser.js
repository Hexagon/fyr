// Small browser helpers for the Maps page (clipboard, file download, file read).
import { localizedError } from '../../i18n/errors.js'

/**
 * Copy text to the clipboard. Falls back to a hidden textarea + execCommand
 * because navigator.clipboard is unavailable on plain-http LAN origins,
 * which is how Fyr is commonly reached off-grid.
 * @returns {Promise<boolean>} true when the copy succeeded.
 */
export const copyText = async (text, { nav = globalThis.navigator, doc = globalThis.document } = {}) => {
  const value = String(text ?? '')
  try {
    if (nav?.clipboard?.writeText) {
      await nav.clipboard.writeText(value)
      return true
    }
  } catch {
    // fall through to legacy path
  }
  if (!doc?.body) return false
  const area = doc.createElement('textarea')
  area.value = value
  area.setAttribute('readonly', '')
  area.style.position = 'fixed'
  area.style.opacity = '0'
  area.style.left = '-9999px'
  doc.body.appendChild(area)
  try {
    area.select()
    return typeof doc.execCommand === 'function' ? Boolean(doc.execCommand('copy')) : false
  } catch {
    return false
  } finally {
    doc.body.removeChild(area)
  }
}

/** Trigger a client-side download of `text` as `filename`. */
export const downloadText = (filename, text, mimeType = 'application/octet-stream', { doc = globalThis.document, url = globalThis.URL } = {}) => {
  const blob = new Blob([text], { type: mimeType })
  const href = url.createObjectURL(blob)
  const a = doc.createElement('a')
  a.href = href
  a.download = filename
  a.rel = 'noopener'
  a.style.display = 'none'
  doc.body.appendChild(a)
  a.click()
  doc.body.removeChild(a)
  setTimeout(() => url.revokeObjectURL(href), 1000)
}

/** Read a File/Blob as text (File.text() with a FileReader fallback). */
export const readFileText = (file) => {
  if (file && typeof file.text === 'function') return file.text()
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(String(reader.result || ''))
    reader.onerror = () => reject(reader.error || localizedError('files.readFailed', 'Could not read file'))
    reader.readAsText(file)
  })
}
