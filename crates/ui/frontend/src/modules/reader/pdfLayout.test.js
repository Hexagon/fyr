import { describe, expect, it } from 'vitest'
import {
  MAX_CANVAS_PIXELS,
  clampPage,
  createLayout,
  fitWidthScale,
  outputScaleFor,
  pageIndexAt,
  positionAt,
  scrollTopFor,
  visibleRange
} from './pdfLayout'

const pages = (count, width = 600, height = 800) => Array.from({ length: count }, () => ({ width, height }))

describe('fitWidthScale', () => {
  it('fills the container width minus padding, times zoom', () => {
    expect(fitWidthScale(624, 600, 1, 12)).toBe(1)
    expect(fitWidthScale(624, 600, 1.5, 12)).toBeCloseTo(1.5)
    expect(fitWidthScale(324, 600, 1, 12)).toBeCloseTo(0.5)
  })

  it('falls back to the zoom when sizes are unknown', () => {
    expect(fitWidthScale(0, 600, 1.2)).toBe(1.2)
    expect(fitWidthScale(500, 0, 1)).toBe(1)
    expect(fitWidthScale(500, 600, Number.NaN)).toBeGreaterThan(0)
  })
})

describe('createLayout / positions', () => {
  const layout = createLayout(pages(5), 0.5, { gap: 10, padding: 10 })

  it('stacks pages with gaps', () => {
    expect(layout.tops).toEqual([10, 420, 830, 1240, 1650])
    expect(layout.heights[0]).toBe(400)
    expect(layout.total).toBe(1650 + 400 + 10)
    expect(layout.maxWidth).toBe(300)
  })

  it('finds the page at a scroll offset', () => {
    expect(pageIndexAt(layout, 0)).toBe(0)
    expect(pageIndexAt(layout, 419)).toBe(0)
    expect(pageIndexAt(layout, 420)).toBe(1)
    expect(pageIndexAt(layout, 99999)).toBe(4)
    expect(pageIndexAt(createLayout([], 1), 10)).toBe(-1)
  })

  it('maps scroll offsets to { page, offset } and back', () => {
    expect(positionAt(layout, 420 + 200)).toEqual({ page: 2, offset: 0.5 })
    expect(scrollTopFor(layout, 2, 0.5)).toBe(620)
    for (const page of [1, 2, 3, 5]) {
      for (const offset of [0, 0.25, 0.9]) {
        const position = positionAt(layout, scrollTopFor(layout, page, offset))
        expect(position.page).toBe(page)
        expect(position.offset).toBeCloseTo(offset, 2)
      }
    }
  })

  it('treats the gap below a page as the start of the next page', () => {
    expect(positionAt(layout, 415)).toEqual({ page: 2, offset: 0 })
    expect(positionAt(layout, 5)).toEqual({ page: 1, offset: 0 })
  })

  it('clamps out-of-range pages and offsets', () => {
    expect(scrollTopFor(layout, 99, 0)).toBe(scrollTopFor(layout, 5, 0))
    expect(scrollTopFor(layout, -3, 2)).toBe(scrollTopFor(layout, 1, 1))
    expect(clampPage('7', 5)).toBe(5)
    expect(clampPage('junk', 5)).toBe(1)
  })

  it('keeps a position stable across zoom (the reason positions are page-relative)', () => {
    const zoomed = createLayout(pages(5), 1.25, { gap: 10, padding: 10 })
    const before = positionAt(layout, 1000)
    const after = positionAt(zoomed, scrollTopFor(zoomed, before.page, before.offset))
    expect(after.page).toBe(before.page)
    expect(after.offset).toBeCloseTo(before.offset, 2)
  })
})

describe('visibleRange', () => {
  it('only covers pages near the viewport, even for huge documents', () => {
    const big = createLayout(pages(800), 0.5, { gap: 10, padding: 10 })
    const top = scrollTopFor(big, 400, 0.5)
    const range = visibleRange(big, top, 700, 1)
    expect(range.first).toBe(399)
    expect(range.last).toBeLessThanOrEqual(403)
    expect(range.last - range.first).toBeLessThan(6)
  })

  it('is clamped at both ends', () => {
    const layout = createLayout(pages(3), 0.5)
    expect(visibleRange(layout, 0, 100, 2)).toEqual({ first: 1, last: 3 })
    expect(visibleRange(createLayout([], 1), 0, 100)).toEqual({ first: 0, last: -1 })
  })
})

describe('outputScaleFor', () => {
  it('uses the device pixel ratio, capped at 2', () => {
    expect(outputScaleFor(400, 500, 1)).toBe(1)
    expect(outputScaleFor(400, 500, 3)).toBe(2)
  })

  it('keeps big canvases under the pixel budget', () => {
    const scale = outputScaleFor(3000, 4000, 2)
    expect(3000 * 4000 * scale * scale).toBeLessThanOrEqual(MAX_CANVAS_PIXELS + 1)
  })
})
