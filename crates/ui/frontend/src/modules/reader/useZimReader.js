import DOMPurify from 'dompurify'
import { ref } from 'vue'

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
    USE_PROFILES: { html: true },
    ADD_TAGS: [
      'figure', 'figcaption', 'main', 'nav', 'header', 'footer',
      'section', 'article', 'aside', 'details', 'summary', 'dialog',
      'data', 'time', 'mark', 'ruby', 'rt', 'rp', 'wbr',
      'center', 'font', 'basefont', 'big', 'small', 'strike', 'tt', 'u',
      'nobr', 'noembed', 'plaintext', 'listing', 'xmp', 'multicol',
      'nextid', 'spacer'
    ],
    ADD_ATTR: [
      'class', 'id', 'style', 'role',
      'colspan', 'rowspan', 'scope', 'headers',
      'align', 'valign', 'width', 'height', 'border',
      'cellpadding', 'cellspacing', 'bgcolor',
      'lang', 'dir', 'title', 'alt',
      'srcset', 'sizes', 'media', 'type', 'rel',
      'target', 'loading', 'decoding', 'fetchpriority',
      'data-*', 'aria-*'
    ],
    ALLOW_DATA_ATTR: true,
    ALLOW_ARIA_ATTR: true,
    FORBID_TAGS: ['script', 'style', 'iframe', 'object', 'embed'],
    FORBID_ATTR: [
      'onerror', 'onload', 'onclick', 'onmouseover',
      'onfocus', 'onblur', 'onchange', 'onsubmit',
      'onreset', 'onselect', 'onkeydown', 'onkeypress', 'onkeyup'
    ]
  })
}

const ALLOWED_ROOT_ATTRS = new Set(['class', 'id', 'lang', 'dir', 'style'])

const escapeHtmlAttribute = (value) => {
  return String(value || '')
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
}

const sanitizeRootStyleValue = (value) => {
  const escaped = escapeHtmlAttribute(value)
  const sanitized = DOMPurify.sanitize(`<div style="${escaped}"></div>`, {
    USE_PROFILES: { html: true },
    ADD_ATTR: ['style'],
    FORBID_TAGS: ['script', 'style', 'iframe', 'object', 'embed'],
    FORBID_ATTR: [
      'onerror', 'onload', 'onclick', 'onmouseover',
      'onfocus', 'onblur', 'onchange', 'onsubmit',
      'onreset', 'onselect', 'onkeydown', 'onkeypress', 'onkeyup'
    ]
  })
  const parser = new DOMParser()
  const doc = parser.parseFromString(sanitized, 'text/html')
  return doc.body.firstElementChild?.getAttribute('style') || ''
}

const extractSanitizedRootAttributes = (element) => {
  const attrs = {}
  if (!element?.attributes) return attrs

  Array.from(element.attributes).forEach((attr) => {
    const name = String(attr.name || '').toLowerCase()
    if (!ALLOWED_ROOT_ATTRS.has(name)) return

    const rawValue = String(attr.value || '').trim()
    if (!rawValue) return

    if (name === 'style') {
      const styleValue = sanitizeRootStyleValue(rawValue)
      if (styleValue) attrs[name] = styleValue
      return
    }

    attrs[name] = rawValue
  })

  return attrs
}

const buildRootAttributeString = (attrs) => {
  const entries = Object.entries(attrs || {})
  if (!entries.length) return ''
  return entries.map(([name, value]) => ` ${name}="${escapeHtmlAttribute(value)}"`).join('')
}

const buildZimSandboxDocument = (headHtml, bodyHtml, rootAttrs = {}) => {
  const readerDefaults =
    '<style>' +
    'a:any-link{color:#0000ee;text-decoration:underline}' +
    'a:visited{color:#551a8b}' +
    '</style>'

  const readerStructure =
    '<style>' +
    'html{height:100%;overflow-x:auto;overflow-y:auto!important;scrollbar-width:auto}' +
    'body{min-height:100%;overflow-x:visible;overflow-y:auto!important;scrollbar-width:auto}' +
    'body,main,article,section,div{scrollbar-width:auto}' +
    'body *{scrollbar-width:auto}' +
    '::-webkit-scrollbar{width:12px;height:12px}' +
    'body *::-webkit-scrollbar{width:12px!important;height:12px!important}' +
    '::-webkit-scrollbar-track{background:#f1f1f1!important}' +
    'body *::-webkit-scrollbar-track{background:#f1f1f1!important}' +
    '::-webkit-scrollbar-thumb{background:#767676!important;border:3px solid #f1f1f1;border-radius:6px}' +
    'body *::-webkit-scrollbar-thumb{background:#767676!important;border:3px solid #f1f1f1;border-radius:6px}' +
    '</style>'

  const navScript =
    '<script>' +
    'document.addEventListener("click", function(e) {' +
    '  if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;' +
    '  var el = e.target;' +
    '  while (el && el.tagName !== "A") { el = el.parentElement; }' +
    '  if (!el) return;' +
    '  var href = el.getAttribute("href");' +
    '  if (!href) return;' +
    '  if (href.charAt(0) === "#") {' +
    '    e.preventDefault();' +
    '    e.stopPropagation();' +
    '    var targetId;' +
    '    try { targetId = decodeURIComponent(href.slice(1)); } catch (ex) { targetId = href.slice(1); }' +
    '    var target = document.getElementById(targetId) || document.getElementsByName(targetId)[0];' +
    '    if (target) target.scrollIntoView();' +
    '    return;' +
    '  }' +
    '  var lower = href.toLowerCase();' +
    '  if (href.indexOf("//") === 0 || /^[a-z][a-z0-9+.-]*:/i.test(href)' +
    '      || lower.indexOf("javascript:") === 0 || lower.indexOf("data:") === 0' +
    '      || lower.indexOf("vbscript:") === 0) return;' +
    '  e.preventDefault();' +
    '  e.stopPropagation();' +
    '  window.parent.postMessage({ type: "zim-navigate", href: href }, "*");' +
    '}, true);' +
    '</script>'

  const htmlAttrs = buildRootAttributeString(rootAttrs.html)
  const bodyAttrs = buildRootAttributeString(rootAttrs.body)

  return (
    `<!DOCTYPE html><html${htmlAttrs}><head>` +
    '<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">' +
    readerDefaults +
    headHtml +
    readerStructure +
    navScript +
    `</head><body${bodyAttrs}>` +
    bodyHtml +
    '</body></html>'
  )
}

const rewriteNativeZimHtml = (filename, articlePath, html, apiService) => {
  // Step 1: Parse the raw HTML to extract CSS and rewrite URLs BEFORE sanitization.
  // DOMPurify strips <style> and <link> tags, so we must extract them first.
  const rawHtml = String(html || '')
  const parser = new DOMParser()
  const doc = parser.parseFromString(rawHtml, 'text/html')

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

    // Detect domain-prefixed ZIM paths (e.g. "www.nhs.uk/medicines/alogliptin/")
    // These are stored as absolute paths in the ZIM archive, not relative to the current article.
    const hasDomainPrefix = /^[a-zA-Z0-9-]+\.[a-zA-Z]{2,}(\/|$)/.test(raw)
    if (hasDomainPrefix) {
      // Treat as an absolute ZIM path - strip any leading slash and use directly
      const normalizedPath = decodePathDeep(raw).replace(/^\/+/, '')
      if (!normalizedPath) return raw
      return normalizedPath
    }

    const resolved = toResolvedUrl(raw, articlePath)
    if (!resolved) return raw
    if (resolved.origin !== window.location.origin) return raw

    const normalizedPath = decodePathDeep(resolved.pathname).replace(/^\/+/, '')
    if (!normalizedPath) {
      return `${resolved.search}${resolved.hash}`
    }
    return `${normalizedPath}${resolved.search}${resolved.hash}`
  }

  // Extract CSS from the raw DOM before sanitization strips it
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

  // Rewrite URLs in the raw DOM
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

  const rootAttrs = {
    html: extractSanitizedRootAttributes(doc.documentElement),
    body: extractSanitizedRootAttributes(doc.body)
  }

  // Step 2: Serialize the body with XMLSerializer to preserve DOM structure
  // (whitespace text nodes, attribute quoting, self-closing tags).
  // doc.body.innerHTML normalizes the HTML, which breaks layouts that depend on
  // precise DOM structure (e.g. Wikipedia mosaic tiles using inline-flex).
  const serializer = new XMLSerializer()
  const rawBodyHtml = doc.body
    ? serializer.serializeToString(doc.body).replace(/^<body[^>]*>/, '').replace(/<\/body>$/, '')
    : ''

  // Step 3: Sanitize only the body HTML (CSS is already extracted in headHtml)
  const bodyHtml = sanitizeNativeZimBodyHtml(rawBodyHtml)

  return {
    headHtml: injectedHeadAssets.join(''),
    bodyHtml,
    rootAttrs
  }
}

export const useZimReader = () => {
  const meta = ref(null)
  const adapter = ref(null)
  const nativeArticle = ref(null)
  const pendingHash = ref(null)

  const dispose = () => {
    meta.value = null
    adapter.value = null
    nativeArticle.value = null
    pendingHash.value = null
  }

  const open = async (descriptor, apiService) => {
    meta.value = await apiService.getZimArchiveMeta(descriptor.filename)
    adapter.value = await apiService
      .getZimReaderCapabilities(descriptor.filename)
      .catch(() => ({
        filename: descriptor.filename,
        mode: 'native',
        supports_native_render: true,
        supports_search: true,
        archive_url: descriptor.content_url
      }))

    if (adapter.value?.supports_native_render) {
      await loadNativeArticle(descriptor.filename, null, apiService)
    }
  }

  const loadNativeArticle = async (filename, path, apiService) => {
    // Extract hash fragment from the path before sending to the server
    let hash = ''
    let cleanPath = path
    if (path) {
      const hashIdx = path.indexOf('#')
      if (hashIdx >= 0) {
        hash = path.slice(hashIdx)
        cleanPath = path.slice(0, hashIdx)
      }
    }
    pendingHash.value = hash || null

    const native = await apiService.getZimNativeArticle(filename, cleanPath)
    const rendered = rewriteNativeZimHtml(
      filename,
      native?.path,
      native?.content,
      apiService
    )

    nativeArticle.value = {
      ...native,
      content: buildZimSandboxDocument(rendered.headHtml, rendered.bodyHtml, rendered.rootAttrs)
    }

    return nativeArticle.value
  }

  return {
    meta,
    adapter,
    nativeArticle,
    pendingHash,
    open,
    loadNativeArticle,
    dispose,
    decodePathDeep
  }
}