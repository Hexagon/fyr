// Lazy loader for pdf.js. Only the PDF reader imports this, and only when a
// PDF is opened, so pdf.js stays out of every other page's bundle.
//
// Everything is served by Fyr itself (Fyr runs offline, there is no CDN):
// - the worker is emitted by Vite as a content-hashed file under
//   /static/assets/ and referenced through `?url` (same pattern as MapLibre's
//   worker in Maps.vue);
// - CMaps, standard fonts, the JPEG 2000/JBIG2 decoders (wasm) and ICC profiles
//   are copied from pdfjs-dist to /static/pdfjs/<version>/ at build time (see
//   vite.config.js) and fetched by pdf.js only when a document needs them.
//
// The legacy build is used on purpose: the modern build relies on very recent
// JavaScript (Map.prototype.getOrInsertComputed, Math.sumPrecise, ...) that
// older phones and tablets on a camp hotspot do not have.

/* global __FYR_PDFJS_ASSET_DIR__ */
const assetDir = typeof __FYR_PDFJS_ASSET_DIR__ === 'string' ? __FYR_PDFJS_ASSET_DIR__ : 'pdfjs/'
const base = `${import.meta.env?.BASE_URL || '/'}${assetDir}`

export const PDFJS_ASSET_BASE = base

let pdfjsPromise = null

export const loadPdfjs = () => {
  if (!pdfjsPromise) {
    pdfjsPromise = Promise.all([
      import('pdfjs-dist/legacy/build/pdf.min.mjs'),
      import('pdfjs-dist/legacy/build/pdf.worker.min.mjs?url')
    ])
      .then(([pdfjs, worker]) => {
        pdfjs.GlobalWorkerOptions.workerSrc = worker.default
        return pdfjs
      })
      .catch((error) => {
        // Allow a retry (e.g. the chunk failed to download on a flaky link).
        pdfjsPromise = null
        throw error
      })
  }
  return pdfjsPromise
}

/** getDocument() parameters for a PDF served at `url`. */
export const documentOptions = (url) => ({
  url,
  cMapUrl: `${base}cmaps/`,
  cMapPacked: true,
  standardFontDataUrl: `${base}standard_fonts/`,
  wasmUrl: `${base}wasm/`,
  iccUrl: `${base}iccs/`,
  enableXfa: false
})
