// Pure layout math for the continuous-scroll PDF viewer (PdfViewer.vue).
//
// Pages are stacked vertically with a fixed gap. Only page sizes are known
// up front (page 1's size is used as the estimate for pages that have not
// been loaded yet), so the layout is rebuilt whenever a real size arrives;
// the viewer keeps the reading position stable across rebuilds by mapping
// scrollTop -> { page, offset } before and back to scrollTop after.
//
// A position is { page, offset }: `page` is 1-based and `offset` is the
// fraction (0..1) of that page's height scrolled past the top of the viewport.
// Being relative to the page, it survives zoom, rotation of the phone and a
// different screen width.

export const PAGE_GAP = 12
export const PAGE_PADDING = 12
// iOS Safari refuses canvases above ~16.7 M pixels; stay well below that so a
// zoomed page on a phone renders blurry rather than blank.
export const MAX_CANVAS_PIXELS = 8 * 1024 * 1024
export const MAX_OUTPUT_SCALE = 2

const finite = (value, fallback) => (Number.isFinite(value) ? value : fallback)

/** Clamp a page number to 1..pageCount (returns 1 for junk). */
export const clampPage = (page, pageCount) => {
  const count = Math.max(1, Math.floor(finite(Number(pageCount), 1)))
  const n = Math.floor(finite(Number(page), 1))
  return Math.min(count, Math.max(1, n))
}

export const clampOffset = (offset) => Math.min(1, Math.max(0, finite(Number(offset), 0)))

/**
 * Scale that makes the widest page fill the container width, times `zoom`.
 * `pageWidth` is the page width at scale 1 (CSS px).
 */
export const fitWidthScale = (containerWidth, pageWidth, zoom = 1, padding = PAGE_PADDING) => {
  const width = finite(Number(containerWidth), 0) - 2 * padding
  const base = finite(Number(pageWidth), 0)
  const z = finite(Number(zoom), 0) > 0 ? Number(zoom) : 1
  if (width <= 0 || base <= 0) return z
  return Math.max(0.1, (width / base) * z)
}

/**
 * Build the vertical layout. `sizes` is an array of { width, height } at
 * scale 1. Returns { tops, heights, widths, total, maxWidth, count }.
 */
export const createLayout = (sizes, scale, { gap = PAGE_GAP, padding = PAGE_PADDING } = {}) => {
  const count = Array.isArray(sizes) ? sizes.length : 0
  const tops = new Array(count)
  const heights = new Array(count)
  const widths = new Array(count)
  const s = finite(Number(scale), 0) > 0 ? Number(scale) : 1
  let y = padding
  let maxWidth = 0
  for (let i = 0; i < count; i += 1) {
    const size = sizes[i] || {}
    const w = Math.max(1, Math.floor(finite(size.width, 1) * s))
    const h = Math.max(1, Math.floor(finite(size.height, 1) * s))
    tops[i] = y
    heights[i] = h
    widths[i] = w
    if (w > maxWidth) maxWidth = w
    y += h + gap
  }
  const total = count ? y - gap + padding : 0
  return { tops, heights, widths, total, maxWidth, count, gap }
}

/** 0-based index of the page whose box (incl. the gap below it) contains y. */
export const pageIndexAt = (layout, y) => {
  if (!layout?.count) return -1
  let lo = 0
  let hi = layout.count - 1
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1
    if (layout.tops[mid] <= y) lo = mid
    else hi = mid - 1
  }
  return lo
}

/** Reading position at scroll offset `y` (the top edge of the viewport). */
export const positionAt = (layout, y) => {
  const index = pageIndexAt(layout, y)
  if (index < 0) return { page: 1, offset: 0 }
  const top = layout.tops[index]
  const height = layout.heights[index]
  // In the gap below a page: that is the start of the next page.
  if (y >= top + height && index + 1 < layout.count) return { page: index + 2, offset: 0 }
  return { page: index + 1, offset: clampOffset((y - top) / height) }
}

/** Scroll offset that puts `{ page, offset }` at the top of the viewport. */
export const scrollTopFor = (layout, page, offset = 0) => {
  if (!layout?.count) return 0
  const index = clampPage(page, layout.count) - 1
  const fraction = clampOffset(offset)
  const top = layout.tops[index] + fraction * layout.heights[index]
  // Page starts get a little breathing room above them (half the gap, so the
  // position still maps back to this page and not the end of the previous one).
  const room = fraction > 0 ? 0 : Math.floor((layout.gap || 0) / 2)
  return Math.max(0, Math.round(top - room))
}

/**
 * Pages (1-based, inclusive) intersecting [scrollTop, scrollTop + height],
 * extended by `overscan` pages on each side.
 */
export const visibleRange = (layout, scrollTop, viewportHeight, overscan = 1) => {
  if (!layout?.count) return { first: 0, last: -1 }
  const top = Math.max(0, finite(scrollTop, 0))
  const bottom = top + Math.max(0, finite(viewportHeight, 0))
  const first = pageIndexAt(layout, top)
  let last = first
  while (last + 1 < layout.count && layout.tops[last + 1] < bottom) last += 1
  return {
    first: Math.max(0, first - overscan) + 1,
    last: Math.min(layout.count - 1, last + overscan) + 1
  }
}

/**
 * Device-pixel scale for a canvas of cssWidth x cssHeight: the device pixel
 * ratio (capped), reduced so the canvas stays under maxPixels.
 */
export const outputScaleFor = (cssWidth, cssHeight, devicePixelRatio = 1, maxPixels = MAX_CANVAS_PIXELS) => {
  const dpr = Math.min(MAX_OUTPUT_SCALE, Math.max(1, finite(Number(devicePixelRatio), 1)))
  const area = Math.max(1, finite(cssWidth, 1) * finite(cssHeight, 1))
  const limit = Math.sqrt(maxPixels / area)
  return Math.max(0.25, Math.min(dpr, limit))
}
