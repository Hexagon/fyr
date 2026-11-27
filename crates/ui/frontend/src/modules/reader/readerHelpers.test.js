import { describe, expect, it, vi } from 'vitest'

// happy-dom's HTML parser makes DOMPurify unwrap the first top-level element
// of every fragment (real browsers and jsdom do not). These tests cover the
// line anchoring, not sanitization, so DOMPurify is replaced by an identity
// function that records what it was asked to clean.
const sanitizeCalls = vi.hoisted(() => [])
vi.mock('dompurify', () => ({
  default: {
    sanitize: (html) => {
      sanitizeCalls.push(String(html))
      return String(html)
    }
  }
}))
import { createArticleHistory } from './articleHistory'
import { isTypingTarget, shortcutForEvent } from './shortcuts'
import { renderMarkdownWithLines } from './useMarkdownReader'
import { findBlockForLine, headingLabelFor } from './markdownPosition'
import { splitArticleHash } from './useZimReader'

describe('article history', () => {
  it('supports back/forward and truncates forward history on visit', () => {
    const h = createArticleHistory()
    h.visit('A')
    h.visit('B')
    h.visit('C')
    expect(h.canForward()).toBe(false)
    expect(h.back()).toBe('B')
    expect(h.back()).toBe('A')
    expect(h.back()).toBeNull()
    expect(h.forward()).toBe('B')
    h.visit('D')
    expect(h.canForward()).toBe(false)
    expect(h.back()).toBe('B')
    expect(h.back()).toBe('A')
  })

  it('ignores consecutive duplicates and empty entries', () => {
    const h = createArticleHistory()
    h.visit('A')
    h.visit('A')
    h.visit('')
    expect(h.size).toBe(1)
    expect(h.canBack()).toBe(false)
  })

  it('caps its size and can restore a pointer after a failed load', () => {
    const h = createArticleHistory(3)
    for (const p of ['1', '2', '3', '4', '5']) h.visit(p)
    expect(h.size).toBe(3)
    const before = h.index
    expect(h.back()).toBe('4')
    h.restore(before)
    expect(h.current()).toBe('5')
    h.reset()
    expect(h.current()).toBeNull()
  })
})

describe('keyboard shortcuts', () => {
  const ev = (key, extra = {}) => ({ key, target: document.body, ...extra })

  it('maps reader keys', () => {
    expect(shortcutForEvent(ev('['))).toBe('history-back')
    expect(shortcutForEvent(ev(']'))).toBe('history-forward')
    expect(shortcutForEvent(ev('b'))).toBe('bookmark')
    expect(shortcutForEvent(ev('/'))).toBe('focus-search')
    expect(shortcutForEvent(ev('+'))).toBe('font-larger')
    expect(shortcutForEvent(ev('-'))).toBe('font-smaller')
    expect(shortcutForEvent(ev('x'))).toBeNull()
  })

  it('never fires while typing or with modifiers', () => {
    const input = document.createElement('input')
    const search = document.createElement('input')
    search.type = 'search'
    const textarea = document.createElement('textarea')
    const select = document.createElement('select')
    const editable = { tagName: 'DIV', isContentEditable: true }
    for (const target of [input, search, textarea, select, editable]) {
      expect(shortcutForEvent(ev('b', { target }))).toBeNull()
      expect(isTypingTarget(target)).toBe(true)
    }
    const checkbox = document.createElement('input')
    checkbox.type = 'checkbox'
    expect(isTypingTarget(checkbox)).toBe(false)
    expect(shortcutForEvent(ev('[', { ctrlKey: true }))).toBeNull()
    expect(shortcutForEvent(ev('[', { altKey: true }))).toBeNull()
    expect(shortcutForEvent(ev('[', { metaKey: true }))).toBeNull()
    expect(shortcutForEvent(ev('[', { defaultPrevented: true }))).toBeNull()
    expect(shortcutForEvent(ev('b', { isComposing: true }))).toBeNull()
    expect(shortcutForEvent(null)).toBeNull()
  })
})

describe('line-anchored markdown', () => {
  const source = ['', '# Title', '', 'Intro text', 'continues', '', '## Section', '', '- a', '- b', '', '<script>alert(1)</script>', ''].join('\n')

  it('annotates top-level blocks with their source line', () => {
    const container = document.createElement('div')
    container.innerHTML = renderMarkdownWithLines(source)
    const lines = Array.from(container.querySelectorAll('[data-md-line]')).map((el) => Number(el.dataset.mdLine))
    expect(lines.slice(0, 4)).toEqual([2, 4, 7, 9])
  })

  it('sanitizes every block separately and never the wrappers', () => {
    sanitizeCalls.length = 0
    renderMarkdownWithLines(source)
    expect(sanitizeCalls.some((html) => html.includes('<script>alert(1)</script>'))).toBe(true)
    expect(sanitizeCalls.every((html) => !html.includes('data-md-line'))).toBe(true)
  })

  it('finds the block for a line and the heading above it', () => {
    const container = document.createElement('div')
    container.innerHTML = renderMarkdownWithLines(source)
    expect(findBlockForLine(container, 5).textContent).toContain('Intro text')
    expect(findBlockForLine(container, 10).textContent).toContain('a')
    expect(findBlockForLine(container, 1).textContent).toContain('Title')
    expect(headingLabelFor(container, findBlockForLine(container, 9))).toBe('Section')
    expect(headingLabelFor(container, findBlockForLine(container, 4))).toBe('Title')
    expect(findBlockForLine(document.createElement('div'), 3)).toBeNull()
  })

  it('handles CRLF line endings', () => {
    const container = document.createElement('div')
    container.innerHTML = renderMarkdownWithLines('# A\r\n\r\ntext\r\n')
    const lines = Array.from(container.querySelectorAll('[data-md-line]')).map((el) => Number(el.dataset.mdLine))
    expect(lines).toEqual([1, 3])
  })
})

describe('splitArticleHash', () => {
  it('splits paths and fragments', () => {
    expect(splitArticleHash('A/Earth#History')).toEqual({ path: 'A/Earth', hash: '#History' })
    expect(splitArticleHash('A/Earth')).toEqual({ path: 'A/Earth', hash: '' })
    expect(splitArticleHash(null)).toEqual({ path: '', hash: '' })
  })
})
