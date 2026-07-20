import { ref } from 'vue'
import { marked } from 'marked'
import DOMPurify from 'dompurify'

export const useMarkdownReader = () => {
  const html = ref('')

  const open = async (descriptor) => {
    const response = await fetch(descriptor.content_url, {
      method: 'GET',
      cache: 'no-store'
    })

    if (!response.ok) {
      throw new Error(`Failed to fetch markdown file (${response.status})`)
    }

    const markdown = await response.text()
    const rendered = marked.parse(markdown)
    html.value = DOMPurify.sanitize(rendered, {
      USE_PROFILES: { html: true }
    })
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
