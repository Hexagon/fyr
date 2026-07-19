import { nextTick, ref } from 'vue'
import EPub from 'epubjs'
import { marked } from 'marked'
import DOMPurify from 'dompurify'

const hasExtension = (filename, extension) => {
  return String(filename || '').toLowerCase().endsWith(extension)
}

const detectFormat = (filename) => {
  if (hasExtension(filename, '.zim')) return 'zim'
  if (hasExtension(filename, '.epub')) return 'epub'
  if (hasExtension(filename, '.md')) return 'md'
  if (hasExtension(filename, '.pdf')) return 'pdf'
  return 'unknown'
}

const inferDescriptorFromSelection = (book) => {
  const format = detectFormat(book?.filename)
  const encoded = encodeURIComponent(book?.filename || '')
  return {
    filename: book?.filename || '',
    format,
    content_url: `/docs/books/${encoded}`,
    meta_url: format === 'zim' ? `/api/reader/zim/${encoded}/meta` : null,
    supports_search: format === 'zim',
    supports_navigation: ['zim', 'epub', 'md', 'pdf'].includes(format),
    supports_inline_render: ['epub', 'md', 'pdf'].includes(format)
  }
}

const buildArticleBaseUrl = (articlePath) => {
  const normalized = `/${String(articlePath || '').replace(/^\/+/, '')}`
  const lastSlash = normalized.lastIndexOf('/')
  const basePath = lastSlash >= 0 ? normalized.slice(0, lastSlash + 1) : '/'
  return new URL(basePath, window.location.origin)
}

const decodePathDeep = (value, maxRounds = 3) => {
  let out = String(value || '')
  for (let i = 0; i < maxRounds; i += 1) {
    try {
      const decoded = decodeURIComponent(out)
      if (decoded === out) break
      out = decoded
    } catch {
      break
    }
  }
  return out
}

const toResolvedUrl = (value, articlePath) => {
  if (!value) return null
  try {
    return new URL(value, buildArticleBaseUrl(articlePath))
  } catch {
    return null
  }
}

const rewriteSrcset = (value, mapUrl) => {
  return String(value || '')
    .split(',')
    .map((candidate) => {
      const token = candidate.trim()
      if (!token) return token

      const firstSpace = token.search(/\s/)
      const urlPart = firstSpace === -1 ? token : token.slice(0, firstSpace)
      const descriptor = firstSpace === -1 ? '' : token.slice(firstSpace)
      const mapped = mapUrl(urlPart)
      return `${mapped}${descriptor}`
    })
    .join(', ')
}

const sanitizeNativeZimBodyHtml = (html) => {
  return DOMPurify.sanitize(html, {
    USE_PROFILES: { html: true }
  })
}

const rewriteNativeZimHtml = (filename, articlePath, html, apiService) => {
  const parser = new DOMParser()
  const doc = parser.parseFromString(String(html || ''), 'text/html')

  const mapAssetUrl = (raw) => {
    if (!raw) return raw
    const lower = String(raw).toLowerCase()
    if (lower.startsWith('data:') || lower.startsWith('blob:') || lower.startsWith('mailto:') || lower.startsWith('javascript:')) {
      return raw
    }

    const resolved = toResolvedUrl(raw, articlePath)
    if (!resolved) return raw
    if (resolved.origin !== window.location.origin) return raw

    const normalizedPath = decodePathDeep(resolved.pathname)
    const base = apiService.getZimNativeContentUrl(filename, normalizedPath)
    return `${base}${resolved.search}${resolved.hash}`
  }

  const mapArticleHref = (raw) => {
    if (!raw || raw.startsWith('#')) return raw
    const lower = String(raw).toLowerCase()
    if (lower.startsWith('mailto:') || lower.startsWith('javascript:')) return raw

    const resolved = toResolvedUrl(raw, articlePath)
    if (!resolved) return raw
    if (resolved.origin !== window.location.origin) return raw

    const normalizedPath = decodePathDeep(resolved.pathname)
    return `${normalizedPath}${resolved.search}${resolved.hash}`
  }

  const injectedHeadAssets = []
  doc.querySelectorAll('link[rel="stylesheet"][href]').forEach((link) => {
    const href = mapAssetUrl(link.getAttribute('href'))
    if (!href) return
    if (!href.startsWith('/api/reader/zim/')) return
    const media = link.getAttribute('media')
    const mediaAttr = media ? ` media="${media}"` : ''
    injectedHeadAssets.push(`<link rel="stylesheet" href="${href}"${mediaAttr}>`)
  })

  doc.querySelectorAll('style').forEach((style) => {
    injectedHeadAssets.push(`<style>${style.textContent || ''}</style>`)
  })

  doc.querySelectorAll('a[href]').forEach((anchor) => {
    const href = anchor.getAttribute('href')
    anchor.setAttribute('href', mapArticleHref(href))
  })

  doc.querySelectorAll('img[src], source[src], video[src], audio[src], track[src]').forEach((node) => {
    const src = node.getAttribute('src')
    node.setAttribute('src', mapAssetUrl(src))
  })

  doc.querySelectorAll('[srcset]').forEach((node) => {
    const srcset = node.getAttribute('srcset')
    node.setAttribute('srcset', rewriteSrcset(srcset, mapAssetUrl))
  })

  return {
    headHtml: injectedHeadAssets.join(''),
    bodyHtml: doc.body.innerHTML
  }
}

export const useUnifiedReader = () => {
  const activeFormat = ref('none')
  const status = ref('idle')
  const error = ref(null)
  const epubBook = ref(null)
  const epubRendition = ref(null)
  const markdownHtml = ref('')
  const zimMeta = ref(null)
  const zimAdapter = ref(null)
  const zimNativeArticle = ref(null)

  const resetContentState = () => {
    markdownHtml.value = ''
    zimMeta.value = null
    zimAdapter.value = null
    zimNativeArticle.value = null

    if (epubRendition.value) {
      try {
        epubRendition.value.destroy()
      } catch {
        // Best-effort cleanup for epubjs internals.
      }
      epubRendition.value = null
    }

    if (epubBook.value) {
      try {
        epubBook.value.destroy()
      } catch {
        // Best-effort cleanup for epubjs internals.
      }
      epubBook.value = null
    }
  }

  const resetForSelection = () => {
    error.value = null
    status.value = 'idle'
    resetContentState()
  }

  const openEpub = async (descriptor) => {
    status.value = `loading ${descriptor.filename}`

    const bookObj = new EPub(descriptor.content_url)
    await bookObj.ready

    await nextTick()

    const rendition = bookObj.renderTo('book-viewer', {
      width: '100%',
      height: '100%',
      manager: 'continuous',
      flow: 'scrolled-doc',
      spread: 'none',
      allowScriptedContent: false
    })

    rendition.flow('scrolled-doc')
    rendition.spread('none')

    epubBook.value = bookObj
    epubRendition.value = rendition

    await rendition.display()

    rendition.themes.default({
      html: {
        'background-color': '#ffffff'
      },
      body: {
        'background-color': '#ffffff',
        color: '#111111'
      }
    })
    rendition.themes.select('default')

    status.value = `opened ${descriptor.filename}`
  }

  const openMarkdown = async (descriptor) => {
    status.value = `loading ${descriptor.filename}`
    const response = await fetch(descriptor.content_url, {
      method: 'GET',
      cache: 'no-store'
    })

    if (!response.ok) {
      throw new Error(`Failed to fetch markdown file (${response.status})`)
    }

    const markdown = await response.text()
    const rendered = marked.parse(markdown)
    markdownHtml.value = DOMPurify.sanitize(rendered, {
      USE_PROFILES: { html: true }
    })

    status.value = `opened ${descriptor.filename}`
  }

  const openZimBootstrap = async (descriptor, apiService) => {
    status.value = `loading ${descriptor.filename}`
    zimMeta.value = await apiService.getZimArchiveMeta(descriptor.filename)
    zimAdapter.value = await apiService
      .getZimReaderCapabilities(descriptor.filename)
      .catch(() => ({
        filename: descriptor.filename,
        mode: 'native',
        supports_native_render: true,
        supports_search: true,
        legacy_bridge_available: false,
        legacy_bridge_url: '',
        archive_url: descriptor.content_url
      }))

    if (zimAdapter.value?.supports_native_render) {
      const nativeArticle = await apiService.getZimNativeArticle(descriptor.filename)
      const rendered = rewriteNativeZimHtml(
        descriptor.filename,
        nativeArticle?.path,
        nativeArticle?.content,
        apiService
      )
      zimNativeArticle.value = {
        ...nativeArticle,
        content: `${rendered.headHtml}\n${sanitizeNativeZimBodyHtml(rendered.bodyHtml)}`
      }
      status.value = `opened ${descriptor.filename}`
      return
    }

    status.value = `metadata ready for ${descriptor.filename}`
  }

  const loadNativeZimArticle = async (filename, path, apiService) => {
    const nativeArticle = await apiService.getZimNativeArticle(filename, path)
    const rendered = rewriteNativeZimHtml(
      filename,
      nativeArticle?.path,
      nativeArticle?.content,
      apiService
    )
    zimNativeArticle.value = {
      ...nativeArticle,
      content: `${rendered.headHtml}\n${sanitizeNativeZimBodyHtml(rendered.bodyHtml)}`
    }
    return zimNativeArticle.value
  }

  const openBook = async (book, apiService) => {
    const descriptor = await apiService
      .getReaderOpenDescriptor(book?.filename)
      .catch(() => inferDescriptorFromSelection(book))

    const format = descriptor?.format || detectFormat(book?.filename)
    activeFormat.value = format
    error.value = null

    resetContentState()

    if (format === 'epub') {
      await openEpub(descriptor)
      return
    }

    if (format === 'md') {
      await openMarkdown(descriptor)
      return
    }

    if (format === 'zim') {
      await openZimBootstrap(descriptor, apiService)
      return
    }

    status.value = 'unsupported format'
  }

  const selectBook = async (book, apiService) => {
    resetForSelection()

    try {
      await openBook(book, apiService)
    } catch (err) {
      error.value = apiService.handleError(err)
      status.value = 'failed'
    }
  }

  const resize = () => {
    epubRendition.value?.resize()
  }

  const dispose = () => {
    resetContentState()
    activeFormat.value = 'none'
    status.value = 'idle'
    error.value = null
  }

  return {
    activeFormat,
    status,
    error,
    epubBook,
    markdownHtml,
    zimMeta,
    zimAdapter,
    zimNativeArticle,
    hasExtension,
    selectBook,
    loadNativeZimArticle,
    resize,
    dispose
  }
}
