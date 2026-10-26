<template>
  <div
    ref="scrollerRef"
    class="pdf-viewer"
    :class="`pdf-theme-${theme}`"
    tabindex="0"
    role="document"
    :aria-label="ariaLabel"
    @scroll.passive="onScroll"
  >
    <div class="pdf-sizer" :style="sizerStyle">
      <div
        v-for="page in pagesInRange"
        :key="page"
        class="pdf-page"
        :data-page="page"
        :style="pageStyle(page)"
      >
        <span class="pdf-page-number" aria-hidden="true">{{ page }}</span>
        <div class="pdf-page-layers"></div>
      </div>
    </div>
  </div>
</template>

<script setup>
// Continuous-scroll PDF viewer on top of pdf.js.
//
// Only the pages near the viewport have DOM nodes (see visibleRange), and only
// those are rendered to canvas, one at a time, nearest to the viewport first.
// Pages that scroll out of range are cancelled and their canvases released,
// so a 500-page PDF costs about as much memory as a 5-page one.
//
// The reading position is { page, offset } (pdfLayout.js). The viewer keeps
// that position stable whenever the layout changes: zoom, a resized window or
// a page whose real size differs from the page-1 estimate.

import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
import {
  clampOffset,
  clampPage,
  createLayout,
  fitWidthScale,
  outputScaleFor,
  positionAt,
  scrollTopFor,
  visibleRange
} from './pdfLayout'
import { buildPageText, findMatches, highlightSegments } from './pdfSearch'
import { loadPdfjs } from './pdfjsLoader'

const props = defineProps({
  doc: { type: Object, default: null },
  theme: { type: String, default: 'light' },
  zoom: { type: Number, default: 1 },
  highlightQuery: { type: String, default: '' },
  initialPosition: { type: Object, default: null },
  label: { type: String, default: 'PDF document' }
})

const emit = defineEmits(['position', 'ready', 'error'])

const scrollerRef = ref(null)
const sizes = shallowRef([])
const baseWidth = ref(0)
const containerWidth = ref(0)
const viewportHeight = ref(0)
const scrollTop = ref(0)

const scale = computed(() => fitWidthScale(containerWidth.value, baseWidth.value, props.zoom))
const layout = computed(() => createLayout(sizes.value, scale.value))
const pagesInRange = computed(() => {
  const { first, last } = visibleRange(layout.value, scrollTop.value, viewportHeight.value, 1)
  const pages = []
  for (let page = first; page >= 1 && page <= last; page += 1) pages.push(page)
  return pages
})

const sizerWidth = computed(() => Math.max(containerWidth.value, layout.value.maxWidth + 2 * layout.value.gap))
const sizerStyle = computed(() => ({ height: `${layout.value.total}px`, width: `${sizerWidth.value}px` }))
const ariaLabel = computed(() => `${props.label}, page ${anchorPage.value} of ${sizes.value.length || 1}`)

const pageStyle = (page) => {
  const l = layout.value
  const index = page - 1
  const width = l.widths[index] || 0
  return {
    top: `${l.tops[index] || 0}px`,
    left: `${Math.max(l.gap, Math.floor((sizerWidth.value - width) / 2))}px`,
    width: `${width}px`,
    height: `${l.heights[index] || 0}px`
  }
}

// Position the user is reading; layout changes scroll back to it.
let anchor = { page: 1, offset: 0 }
const anchorPage = ref(1)
let docGeneration = 0
let ready = false
let pdfjs = null
let resizeObserver = null
let scrollFrame = 0
let emitTimer = null
let pendingSizes = null
let activeMatch = null // { page, occurrence }
let pendingReveal = null // { page, occurrence }

/** page -> { el, canvas, renderTask, textLayer, textDiv, pageText, scale, failed, marked } */
const pageStates = new Map()
let pumping = false
let pumpQueued = false

const setAnchor = (position) => {
  anchor = { page: position.page, offset: position.offset }
  anchorPage.value = position.page
}

const applyScroll = (top) => {
  const scroller = scrollerRef.value
  if (!scroller) return
  scroller.scrollTop = top
  scrollTop.value = scroller.scrollTop
}

const scrollToAnchor = () => applyScroll(scrollTopFor(layout.value, anchor.page, anchor.offset))

// ---------------------------------------------------------------------------
// Scrolling and position reporting
// ---------------------------------------------------------------------------

const emitPosition = () => {
  if (emitTimer) clearTimeout(emitTimer)
  emitTimer = setTimeout(() => {
    emitTimer = null
    if (ready) emit('position', { ...anchor })
  }, 250)
}

const onScroll = () => {
  if (scrollFrame) return
  scrollFrame = requestAnimationFrame(() => {
    scrollFrame = 0
    const scroller = scrollerRef.value
    if (!scroller || !ready) return
    scrollTop.value = scroller.scrollTop
    setAnchor(positionAt(layout.value, scroller.scrollTop))
    emitPosition()
  })
}

const measure = () => {
  const scroller = scrollerRef.value
  if (!scroller) return
  containerWidth.value = scroller.clientWidth
  viewportHeight.value = scroller.clientHeight
}

// Layout changed (zoom, resize, real page sizes): keep the reading position.
watch(layout, () => {
  if (ready) scrollToAnchor()
  // Text layers size their font from this variable; keep old ones aligned
  // until the page is re-rendered at the new scale.
  for (const state of pageStates.values()) {
    state.textDiv?.style.setProperty('--total-scale-factor', String(scale.value))
  }
}, { flush: 'post' })

// ---------------------------------------------------------------------------
// Page rendering
// ---------------------------------------------------------------------------

const releaseCanvas = (canvas) => {
  if (!canvas) return
  // Shrinking the backing store frees GPU/bitmap memory right away (Safari
  // keeps detached canvases alive for a while otherwise).
  canvas.width = 0
  canvas.height = 0
  canvas.remove()
}

const destroyPageState = (page) => {
  const state = pageStates.get(page)
  if (!state) return
  pageStates.delete(page)
  state.destroyed = true
  try {
    state.renderTask?.cancel()
  } catch {
    // already finished
  }
  try {
    state.textLayer?.cancel()
  } catch {
    // already finished
  }
  releaseCanvas(state.canvas)
  state.textDiv?.remove()
}

const queueSizeUpdate = (page, width, height) => {
  const current = (pendingSizes || sizes.value)[page - 1]
  if (current && Math.abs(current.width - width) < 0.5 && Math.abs(current.height - height) < 0.5) return
  if (!pendingSizes) {
    pendingSizes = sizes.value.slice()
    queueMicrotask(() => {
      const next = pendingSizes
      pendingSizes = null
      if (next && next.length === sizes.value.length) sizes.value = next
    })
  }
  pendingSizes[page - 1] = { width, height }
  if (width > baseWidth.value) baseWidth.value = width
}

const showPageError = (state, page) => {
  if (!state.el || state.el.querySelector('.pdf-page-error')) return
  const note = document.createElement('p')
  note.className = 'pdf-page-error'
  note.textContent = `Page ${page} could not be displayed.`
  state.el.append(note)
}

const applyHighlights = (page, state) => {
  const divs = state.textLayer?.textDivs
  const pageText = state.pageText
  if (!divs || !pageText) return
  for (const i of state.marked) {
    if (divs[i]) divs[i].textContent = pageText.strs[i]
  }
  state.marked.clear()

  const query = props.highlightQuery
  if (!query) return
  const matches = findMatches(pageText.text, query)
  if (!matches.length) return
  const selected = activeMatch && activeMatch.page === page ? activeMatch.occurrence : -1
  for (const [i, parts] of highlightSegments(pageText.strs, pageText.starts, matches, selected)) {
    const div = divs[i]
    if (!div) continue
    div.textContent = ''
    for (const part of parts) {
      if (part.match < 0) {
        div.append(document.createTextNode(part.text))
      } else {
        const mark = document.createElement('span')
        mark.className = part.selected ? 'highlight selected' : 'highlight'
        mark.textContent = part.text
        div.append(mark)
      }
    }
    state.marked.add(i)
  }

  if (pendingReveal && pendingReveal.page === page) {
    pendingReveal = null
    revealSelected(page, state)
  }
}

const revealSelected = (page, state) => {
  const scroller = scrollerRef.value
  const target = state.el?.querySelector('.highlight.selected')
  if (!scroller || !target) return
  const box = scroller.getBoundingClientRect()
  const rect = target.getBoundingClientRect()
  const matchTop = scroller.scrollTop + (rect.top - box.top)
  // Put the match a third down the viewport, but do not scroll above the
  // start of its page when the match is near the top (the page stays current).
  const pageStart = scrollTopFor(layout.value, page, 0)
  let top = matchTop - scroller.clientHeight / 3
  if (top < pageStart && matchTop - pageStart < scroller.clientHeight * 0.8) top = pageStart
  const left = scroller.scrollLeft + (rect.left - box.left) - scroller.clientWidth / 2
  applyScroll(Math.max(0, top))
  if (scroller.scrollWidth > scroller.clientWidth) scroller.scrollLeft = Math.max(0, left)
  setAnchor(positionAt(layout.value, scroller.scrollTop))
  emitPosition()
}

const renderPage = async (page, state) => {
  const doc = props.doc
  const generation = docGeneration
  const targetScale = scale.value
  const isStale = () => generation !== docGeneration || state.destroyed || pageStates.get(page) !== state
  state.rendering = true
  try {
    const pdfPage = await doc.getPage(page)
    if (isStale()) return

    const unscaled = pdfPage.getViewport({ scale: 1 })
    queueSizeUpdate(page, unscaled.width, unscaled.height)

    const viewport = pdfPage.getViewport({ scale: targetScale })
    const outputScale = outputScaleFor(viewport.width, viewport.height, window.devicePixelRatio || 1)
    const canvas = document.createElement('canvas')
    canvas.className = 'pdf-canvas'
    canvas.width = Math.max(1, Math.floor(viewport.width * outputScale))
    canvas.height = Math.max(1, Math.floor(viewport.height * outputScale))
    const task = pdfPage.render({
      canvasContext: canvas.getContext('2d', { alpha: false }),
      canvas,
      viewport,
      transform: outputScale !== 1 ? [outputScale, 0, 0, outputScale, 0, 0] : null
    })
    state.renderTask = task
    try {
      await task.promise
    } catch (error) {
      releaseCanvas(canvas)
      if (error?.name === 'RenderingCancelledException' || isStale()) return
      state.failed = true
      showPageError(state, page)
      return
    } finally {
      if (state.renderTask === task) state.renderTask = null
    }
    if (isStale()) {
      releaseCanvas(canvas)
      return
    }

    const previous = state.canvas
    state.el.prepend(canvas)
    state.canvas = canvas
    releaseCanvas(previous)
    state.scale = targetScale

    // Text layer (selection, copy, search highlights).
    const textContent = await pdfPage.getTextContent()
    if (isStale() || state.scale !== targetScale) return
    try {
      state.textLayer?.cancel()
    } catch {
      // finished
    }
    state.textDiv?.remove()
    const textDiv = document.createElement('div')
    textDiv.className = 'textLayer'
    textDiv.style.setProperty('--total-scale-factor', String(targetScale))
    state.el.append(textDiv)
    const textLayer = new pdfjs.TextLayer({ textContentSource: textContent, container: textDiv, viewport })
    state.textDiv = textDiv
    state.textLayer = textLayer
    state.marked = new Set()
    state.pageText = buildPageText(textContent)
    await textLayer.render()
    if (isStale() || state.textLayer !== textLayer) return
    applyHighlights(page, state)
  } catch (error) {
    if (isStale() || error?.name === 'AbortException' || error?.name === 'RenderingCancelledException') return
    state.failed = true
    showPageError(state, page)
  } finally {
    state.rendering = false
  }
}

const nextPageToRender = () => {
  const center = anchor.page
  let best = null
  let bestDistance = Infinity
  for (const [page, state] of pageStates) {
    if (state.failed || state.rendering || !state.el) continue
    if (state.scale === scale.value) continue
    const distance = Math.abs(page - center)
    if (distance < bestDistance) {
      best = page
      bestDistance = distance
    }
  }
  return best
}

const pump = async () => {
  pumpQueued = false
  if (pumping || !ready) return
  pumping = true
  try {
    for (;;) {
      const page = nextPageToRender()
      if (page === null) break
      await renderPage(page, pageStates.get(page))
    }
  } finally {
    pumping = false
  }
}

const schedulePump = () => {
  if (pumpQueued) return
  pumpQueued = true
  setTimeout(pump, 0)
}

const syncPageStates = () => {
  const scroller = scrollerRef.value
  if (!scroller) return
  const wanted = new Set(pagesInRange.value)
  for (const page of Array.from(pageStates.keys())) {
    if (!wanted.has(page)) destroyPageState(page)
  }
  for (const pageEl of scroller.querySelectorAll('.pdf-page[data-page]')) {
    const page = Number(pageEl.dataset.page)
    const layers = pageEl.querySelector('.pdf-page-layers')
    const state = pageStates.get(page)
    if (state) {
      state.el = layers
    } else {
      pageStates.set(page, { el: layers, scale: 0, marked: new Set() })
    }
  }
  schedulePump()
}

watch([pagesInRange, scale], syncPageStates, { flush: 'post' })

const refreshHighlights = () => {
  for (const [page, state] of pageStates) applyHighlights(page, state)
}

watch(() => props.highlightQuery, () => {
  if (!props.highlightQuery) activeMatch = null
  refreshHighlights()
})

// ---------------------------------------------------------------------------
// Document lifecycle
// ---------------------------------------------------------------------------

const resetDocument = () => {
  docGeneration += 1
  ready = false
  activeMatch = null
  pendingReveal = null
  for (const page of Array.from(pageStates.keys())) destroyPageState(page)
  sizes.value = []
  baseWidth.value = 0
  pendingSizes = null
  setAnchor({ page: 1, offset: 0 })
}

watch(
  () => props.doc,
  async (doc) => {
    resetDocument()
    if (!doc) return
    const generation = docGeneration
    try {
      const [lib, first] = await Promise.all([loadPdfjs(), doc.getPage(1)])
      if (generation !== docGeneration) return
      pdfjs = lib
      const viewport = first.getViewport({ scale: 1 })
      baseWidth.value = viewport.width
      const estimate = { width: viewport.width, height: viewport.height }
      sizes.value = Array.from({ length: doc.numPages }, () => estimate)
    } catch (error) {
      if (generation !== docGeneration) return
      emit('error', error)
      return
    }
    await nextTick()
    if (generation !== docGeneration) return
    measure()
    const start = props.initialPosition
    setAnchor({
      page: clampPage(start?.page ?? 1, doc.numPages),
      offset: clampOffset(start?.offset ?? 0)
    })
    ready = true
    await nextTick()
    scrollToAnchor()
    syncPageStates()
    emit('ready')
  },
  { immediate: true }
)

// ---------------------------------------------------------------------------
// Public API (used by Books.vue through a template ref)
// ---------------------------------------------------------------------------

/** Scroll to `page` (1-based) at `offset` (0..1 of the page height). */
const goTo = (page, offset = 0) => {
  if (!ready) return false
  setAnchor({ page: clampPage(page, sizes.value.length), offset: clampOffset(offset) })
  scrollToAnchor()
  emitPosition()
  return true
}

const currentPosition = () => (ready ? { ...anchor } : null)

/** Select search match `occurrence` on `page`, scroll to it and highlight it. */
const showMatch = (page, occurrence = 0) => {
  if (!ready) return false
  activeMatch = { page, occurrence }
  pendingReveal = { page, occurrence }
  goTo(page, 0)
  refreshHighlights()
  return true
}

const focus = () => scrollerRef.value?.focus({ preventScroll: true })

defineExpose({ goTo, currentPosition, showMatch, focus })

onMounted(() => {
  measure()
  if (typeof ResizeObserver !== 'undefined' && scrollerRef.value) {
    resizeObserver = new ResizeObserver(() => measure())
    resizeObserver.observe(scrollerRef.value)
  }
})

onBeforeUnmount(() => {
  // The parent reads currentPosition() before closing a book, so a pending
  // position event can simply be dropped here.
  resizeObserver?.disconnect()
  if (scrollFrame) cancelAnimationFrame(scrollFrame)
  if (emitTimer) clearTimeout(emitTimer)
  emitTimer = null
  resetDocument()
})
</script>

<style scoped>
.pdf-viewer {
  position: relative;
  overflow: auto;
  overscroll-behavior: contain;
  scrollbar-gutter: stable;
  -webkit-overflow-scrolling: touch;
  outline: none;
}

.pdf-viewer:focus-visible {
  box-shadow: inset 0 0 0 2px #40c0b5;
}

.pdf-sizer {
  position: relative;
  min-width: 100%;
}

.pdf-page {
  position: absolute;
  background: #ffffff;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.25);
  overflow: hidden;
}

.pdf-theme-sepia .pdf-page {
  background: #f4ecd8;
}

.pdf-theme-dark .pdf-page {
  background: #1d2125;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.6);
}

.pdf-page-number {
  position: absolute;
  top: 50%;
  left: 0;
  right: 0;
  text-align: center;
  font-size: 0.9rem;
  color: #8a949e;
}

.pdf-page-layers {
  position: absolute;
  inset: 0;
}

.pdf-page-layers :deep(.pdf-canvas) {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  display: block;
}

/* Themes recolor the rendered page; the text layer is transparent. */
.pdf-theme-sepia .pdf-page-layers :deep(.pdf-canvas) {
  filter: sepia(0.55) saturate(1.15) brightness(0.96);
}

.pdf-theme-dark .pdf-page-layers :deep(.pdf-canvas) {
  filter: invert(0.88) hue-rotate(180deg);
}

.pdf-page-layers :deep(.pdf-page-error) {
  position: absolute;
  inset: auto 0 45% 0;
  margin: 0;
  text-align: center;
  color: #a2332f;
  font-size: 0.9rem;
}

/* pdf.js text layer (subset of pdfjs-dist/web/pdf_viewer.css). Width and
   height are forced to the page box so browsers without CSS round() work. */
.pdf-page-layers :deep(.textLayer) {
  position: absolute;
  inset: 0;
  width: 100% !important;
  height: 100% !important;
  overflow: clip;
  opacity: 1;
  line-height: 1;
  text-align: initial;
  letter-spacing: normal;
  word-spacing: normal;
  -webkit-text-size-adjust: none;
  text-size-adjust: none;
  forced-color-adjust: none;
  transform-origin: 0 0;
  caret-color: CanvasText;
  z-index: 0;
  --min-font-size: 1;
  --text-scale-factor: calc(var(--total-scale-factor) * var(--min-font-size));
  --min-font-size-inv: calc(1 / var(--min-font-size));
}

.pdf-page-layers :deep(.textLayer span),
.pdf-page-layers :deep(.textLayer br) {
  color: transparent;
  position: absolute;
  white-space: pre;
  cursor: text;
  transform-origin: 0% 0%;
  user-select: text;
  -webkit-user-select: text;
}

.pdf-page-layers :deep(.textLayer > :not(.markedContent)),
.pdf-page-layers :deep(.textLayer .markedContent span:not(.markedContent)) {
  z-index: 1;
  --font-height: 0;
  font-size: calc(var(--text-scale-factor) * var(--font-height));
  --scale-x: 1;
  --rotate: 0deg;
  transform: rotate(var(--rotate)) scaleX(var(--scale-x)) scale(var(--min-font-size-inv));
}

.pdf-page-layers :deep(.textLayer .markedContent) {
  display: contents;
}

.pdf-page-layers :deep(.textLayer ::selection) {
  background: rgba(0, 90, 255, 0.3);
  color: transparent;
}

.pdf-page-layers :deep(.textLayer br::selection) {
  background: transparent;
}

/* Search hits: nested inside a text span, so they inherit its transform. */
.pdf-page-layers :deep(.textLayer .highlight) {
  position: static;
  transform: none;
  font-size: inherit;
  margin: -1px;
  padding: 1px;
  border-radius: 3px;
  background-color: rgba(255, 196, 0, 0.42);
}

.pdf-page-layers :deep(.textLayer .highlight.selected) {
  background-color: rgba(255, 98, 0, 0.55);
  box-shadow: 0 0 0 2px rgba(255, 98, 0, 0.55);
}
</style>
