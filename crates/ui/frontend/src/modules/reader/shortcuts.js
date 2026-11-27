// Reader keyboard shortcuts. Kept pure so the "never hijack typing" rule is
// unit tested. Key events inside the EPUB/ZIM iframes do not reach the page,
// so shortcuts apply while focus is on the Books page itself.

const TYPING_INPUT_TYPES = new Set([
  'text', 'search', 'email', 'url', 'tel', 'password', 'number',
  'date', 'datetime-local', 'month', 'time', 'week', 'color', ''
])

/** True when key presses on `el` are text entry and must not trigger shortcuts. */
export const isTypingTarget = (el) => {
  if (!el || typeof el !== 'object') return false
  if (el.isContentEditable) return true
  const tag = String(el.tagName || '').toUpperCase()
  if (tag === 'TEXTAREA' || tag === 'SELECT') return true
  if (tag === 'INPUT') {
    const type = String(el.getAttribute?.('type') ?? el.type ?? '').toLowerCase()
    return TYPING_INPUT_TYPES.has(type)
  }
  return false
}

/**
 * Map a keydown event to a reader action name, or null.
 *   [ / ]   ZIM article back / forward
 *   b       add bookmark at the current position
 *   /       focus the in-book search box
 *   + / -   larger / smaller text (EPUB, Markdown)
 */
export const shortcutForEvent = (event) => {
  if (!event || event.defaultPrevented || event.isComposing) return null
  if (event.ctrlKey || event.metaKey || event.altKey) return null
  if (isTypingTarget(event.target)) return null

  switch (event.key) {
    case '[':
      return 'history-back'
    case ']':
      return 'history-forward'
    case 'b':
    case 'B':
      return event.shiftKey ? null : 'bookmark'
    case '/':
      return 'focus-search'
    case '+':
    case '=':
      return 'font-larger'
    case '-':
    case '_':
      return 'font-smaller'
    default:
      return null
  }
}
