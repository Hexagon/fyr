import { ref } from 'vue'
import { marked } from 'marked'
import DOMPurify from 'dompurify'

const countNewlines = (text) => {
  let count = 0
  for (let i = 0; i < text.length; i += 1) {
    if (text.charCodeAt(i) === 10) count += 1
  }
  return count
}

/**
 * Render markdown to sanitized HTML where every top-level block is wrapped in
 * `<div class="md-block" data-md-line="N">` (N = 1-based source line where the
 * block starts). The line anchors let the reader restore positions, jump to
 * bookmarks and jump to `/api/library/books/:f/search` hits (which report
 * markdown matches as `#<line>`).
 */
export const renderMarkdownWithLines = (markdown) => {
  const source = String(markdown || '').replace(/\r\n?/g, '\n')
  const tokens = marked.lexer(source)
  let line = 1
  const parts = []

  for (const token of tokens) {
    const raw = typeof token.raw === 'string' ? token.raw : ''
    // Leading blank lines inside a token's raw text belong before the block.
    const leading = raw.match(/^\n*/)[0].length
    const startLine = line + leading
    line += countNewlines(raw)

    if (token.type === 'space') continue

    const single = [token]
    single.links = tokens.links
    // Sanitize each block on its own and add the wrapper afterwards: the
    // wrapper attributes are ours (an integer line), and an unbalanced raw
    // HTML block cannot swallow or close the wrappers of other blocks.
    const html = DOMPurify.sanitize(marked.parser(single), {
      USE_PROFILES: { html: true }
    })
    if (!html.trim()) continue
    parts.push(`<div class="md-block" data-md-line="${startLine}">${html}</div>`)
  }

  return parts.join('')
}

export const useMarkdownReader = () => {
  const html = ref('')

  const open = async (descriptor, token = null) => {
    const response = await fetch(descriptor.content_url, {
      method: 'GET',
      cache: 'no-store'
    })

    if (!response.ok) {
      throw new Error(`Failed to fetch markdown file (${response.status})`)
    }

    const markdown = await response.text()
    if (token?.cancelled) return
    html.value = renderMarkdownWithLines(markdown)
  }

  const dispose = () => {
    html.value = ''
  }

  return {
    html,
    open,
    dispose
  }
}
