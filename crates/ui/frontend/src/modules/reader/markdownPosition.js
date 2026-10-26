// DOM helpers for line-anchored Markdown (see renderMarkdownWithLines).
// The Markdown pane scrolls with the window, so positions are expressed as the
// source line of the top-most visible block.

const blocksOf = (container) =>
  container ? Array.from(container.querySelectorAll('[data-md-line]')) : []

const lineOf = (el) => Number.parseInt(el?.getAttribute?.('data-md-line') || '', 10)

/** Last block that starts at or before `line` (or the first block). */
export const findBlockForLine = (container, line) => {
  const blocks = blocksOf(container)
  if (!blocks.length) return null
  const target = Number(line)
  if (!Number.isFinite(target)) return blocks[0]
  let match = blocks[0]
  for (const block of blocks) {
    const start = lineOf(block)
    if (Number.isFinite(start) && start <= target) match = block
    else if (Number.isFinite(start) && start > target) break
  }
  return match
}

/**
 * First block whose bottom edge is below `viewportTop` (a client-rect Y,
 * usually the bottom of the sticky header or 0). Returns its line or null.
 */
export const topVisibleLine = (container, viewportTop = 0) => {
  const blocks = blocksOf(container)
  for (const block of blocks) {
    const rect = block.getBoundingClientRect()
    if (rect.bottom > viewportTop + 1) {
      const line = lineOf(block)
      return Number.isFinite(line) ? line : null
    }
  }
  return null
}

/** Text of the nearest heading at or before `block`, for bookmark labels. */
export const headingLabelFor = (container, block) => {
  const blocks = blocksOf(container)
  const idx = block ? blocks.indexOf(block) : -1
  for (let i = idx; i >= 0; i -= 1) {
    const heading = blocks[i].querySelector('h1, h2, h3, h4, h5, h6')
    if (heading) return heading.textContent.trim().slice(0, 200)
  }
  return ''
}
