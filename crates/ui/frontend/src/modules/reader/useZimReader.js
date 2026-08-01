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

// Moderate fallback styles for ZIM content that lacks its own CSS.
// These provide Wikipedia-compatible defaults (blue links, mosaic grid,
// thumbnails, infoboxes) while being low-specificity so ZIM's own extracted
// CSS (injected AFTER this) naturally overrides them.
// No !important (except overflow-x which is structural), no universal selectors.
const ZIM_SANDBOX_BASE_CSS = [
  'html,body{background:#fff;margin:0;padding:0;overflow-x:hidden!important;overflow-y:visible;overscroll-behavior:contain}',
  'body{font-family:sans-serif;font-size:1rem;line-height:1.6;color:#202122}',
  'a{color:#3366cc;text-decoration:none}',
  'a:visited{color:#6b4ba1}',
  'a:hover{text-decoration:underline}',
  'h1,h2,h3,h4{font-family:"Linux Libertine","Times New Roman",Times,serif;font-weight:500;line-height:1.25;border-bottom:1px solid #eaecf0;margin:1rem 0 0.6rem;padding-bottom:0.15rem}',
  'p{margin:0.45rem 0 0.7rem}',
  'ul,ol{margin:0.55rem 0 0.85rem 1.25rem}',
  'li{margin-bottom:0.25rem}',
  'img{max-width:100%;height:auto}',
  'figure{margin:0}',
  'figcaption{font-size:0.78rem;line-height:1.3;color:#3b3f45}',
  'table{max-width:100%;border-collapse:collapse}',
  'td,th{border:1px solid #d8dde3;padding:0.32rem 0.45rem;vertical-align:top}',
  '.thumb{max-width:100%}',
  '.thumb img{border:1px solid #c8ccd1;padding:2px;background:#fff}',
  '.infobox{float:right;max-width:min(320px,100%);margin:0 0 0.8rem 0.9rem;font-size:0.86rem;background:#f8f9fa;border-spacing:0}',
  '.infobox td,.infobox th{border:1px solid #c8ccd1;padding:0.32rem 0.45rem}'
].join('')

const buildZimSandboxDocument = (headHtml, bodyHtml) => {
  const navScript =
    '<script>' +
    'function fyrNotifyHeight() {' +
    '  var body = document.body;' +
    '  var docEl = document.documentElement;' +
    '  var height = Math.max(' +
    '    body ? body.scrollHeight : 0,' +
    '    body ? body.offsetHeight : 0,' +
    '    docEl ? docEl.scrollHeight : 0,' +
    '    docEl ? docEl.offsetHeight : 0' +
    '  );' +
    '  window.parent.postMessage({ type: "zim-height", height: height }, "*");' +
    '}' +
    'document.addEventListener("click", function(e) {' +
    '  if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;' +
    '  var el = e.target;' +
    '  while (el && el.tagName !== "A") { el = el.parentElement; }' +
    '  if (!el) return;' +
    '  var href = el.getAttribute("href");' +
    '  if (!href || href.charAt(0) === "#") return;' +
    '  var lower = href.toLowerCase();' +
    '  if (lower.indexOf("mailto:") === 0 || lower.indexOf("javascript:") === 0' +
    '      || lower.indexOf("data:") === 0 || lower.indexOf("vbscript:") === 0) return;' +
    '  try {' +
    '    var url = new URL(href, location.href);' +
    '    if (url.origin !== location.origin) return;' +
    '  } catch (ex) { return; }' +
    '  e.preventDefault();' +
    '  e.stopPropagation();' +
    '  window.parent.postMessage({ type: "zim-navigate", href: href }, "*");' +
    '}, true);' +
    'window.addEventListener("load", fyrNotifyHeight);' +
    'window.addEventListener("resize", fyrNotifyHeight);' +
    'new MutationObserver(function() { fyrNotifyHeight(); }).observe(document.documentElement, { childList: true, subtree: true, attributes: true, characterData: true });' +
    'setTimeout(fyrNotifyHeight, 0);' +
    'setTimeout(fyrNotifyHeight, 300);' +
    'setTimeout(fyrNotifyHeight, 1200);' +
    '</script>'

  // Order: base reset/fallback first, then ZIM's own extracted CSS.
  // This lets the ZIM's styles naturally override the fallback.
  return (
    '<!DOCTYPE html><html><head>' +
    '<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">' +
    `<style>${ZIM_SANDBOX_BASE_CSS}</style>` +
    headHtml +
    navScript +
    '</head><body>' +
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
    bodyHtml
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
        legacy_bridge_available: false,
        legacy_bridge_url: '',
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
      content: buildZimSandboxDocument(rendered.headHtml, rendered.bodyHtml)
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