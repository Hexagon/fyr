import { markRaw, ref, shallowRef } from 'vue'
import { documentOptions, loadPdfjs as defaultLoadPdfjs } from './pdfjsLoader'
import { buildPageText } from './pdfSearch'

/** User-facing message for a pdf.js load failure. */
export const describePdfError = (error) => {
  const name = error?.name || ''
  if (name === 'PasswordException') {
    return 'This PDF is password-protected (encrypted) and cannot be opened in the reader. Download the file and open it in an app that can unlock it.'
  }
  if (name === 'InvalidPDFException' || name === 'FormatError') {
    return 'This file is not a valid PDF or it is damaged, so it cannot be shown.'
  }
  if (name === 'ResponseException' || name === 'MissingPDFException') {
    const status = Number(error?.status)
    if (status === 404 || error?.missing) return 'The PDF file was not found on the server. It may have been removed.'
    return `The PDF could not be downloaded${status ? ` (HTTP ${status})` : ''}.`
  }
  const message = String(error?.message || '').trim()
  return message ? `Could not open this PDF: ${message}` : 'Could not open this PDF.'
}

/**
 * pdf.js document lifecycle for the Books reader.
 *
 * `open()` follows the unified reader's cancellation token: when the user
 * switches books, useUnifiedReader cancels the token and calls `dispose()`,
 * which destroys the in-flight loading task (aborting the download and the
 * worker). A load that finishes late is destroyed instead of shown.
 *
 * The document proxy is kept in a shallowRef and marked raw: pdf.js objects
 * use private class fields, which break behind Vue's reactive proxies.
 */
export const usePdfReader = ({ loadPdfjs = defaultLoadPdfjs } = {}) => {
  const url = ref('')
  const doc = shallowRef(null)
  const pageCount = ref(0)

  let loadingTask = null
  let generation = 0
  let textCache = new Map()

  const release = () => {
    generation += 1
    textCache = new Map()
    const task = loadingTask
    loadingTask = null
    doc.value = null
    pageCount.value = 0
    if (task) {
      // destroy() also terminates the worker created for this document.
      Promise.resolve()
        .then(() => task.destroy())
        .catch(() => {})
    }
  }

  const open = async (descriptor, token = null) => {
    release()
    const gen = generation
    const isStale = () => gen !== generation || Boolean(token?.cancelled)
    url.value = descriptor?.content_url || ''

    const pdfjs = await loadPdfjs()
    if (isStale()) return

    const task = pdfjs.getDocument(documentOptions(url.value))
    loadingTask = task

    let pdf
    try {
      pdf = await task.promise
    } catch (error) {
      if (isStale()) return
      loadingTask = null
      Promise.resolve()
        .then(() => task.destroy())
        .catch(() => {})
      throw new Error(describePdfError(error))
    }
    if (isStale()) {
      // A newer selection (or dispose) won; free this document.
      if (loadingTask === task) loadingTask = null
      Promise.resolve()
        .then(() => task.destroy())
        .catch(() => {})
      return
    }
    doc.value = markRaw(pdf)
    pageCount.value = pdf.numPages
  }

  /**
   * Searchable text of page `n` (cached per document; only the strings are
   * kept, not pdf.js' full text content with glyph geometry). Resolves to
   * buildPageText()'s { text, strs, starts }, or null without a document.
   */
  const getPageText = (n) => {
    const pdf = doc.value
    if (!pdf) return Promise.resolve(null)
    const cache = textCache
    if (cache.has(n)) return cache.get(n)
    const promise = pdf
      .getPage(n)
      .then((page) => page.getTextContent())
      .then((textContent) => buildPageText(textContent))
      .catch((error) => {
        cache.delete(n)
        throw error
      })
    cache.set(n, promise)
    return promise
  }

  const dispose = () => {
    release()
    url.value = ''
  }

  return {
    url,
    doc,
    pageCount,
    open,
    getPageText,
    dispose
  }
}
