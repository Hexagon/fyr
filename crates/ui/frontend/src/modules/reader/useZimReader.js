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
    USE_PROFILES: { html: true }
  })
}

const ZIM_ARTICLE_BASE_CSS = [
  'html,body{height:auto;min-height:100%;overflow:hidden!important}',
  'html{background:#ffffff}',
  'body{margin:0;padding:1.25rem;background:#ffffff;color:#202122;',
  'font-family:Georgia,"Times New Roman",Times,serif;font-size:0.98rem;',
  'line-height:1.62;overflow-x:hidden}',
  'a{color:#3366cc;text-decoration:none}',
  'a:hover{color:#003399;text-decoration:underline}',
  'p{margin:0.45rem 0 0.7rem}',
  'h1,h2,h3,h4{font-family:"Linux Libertine","Times New Roman",Times,serif;',
  'font-weight:500;line-height:1.25;border-bottom:1px solid #eaecf0;',
  'margin:1rem 0 0.6rem;padding-bottom:0.15rem}',
  'ul,ol{margin:0.55rem 0 0.85rem 1.25rem}',
  'li{margin-bottom:0.25rem}',
  'img{max-width:100%;height:auto;display:block}',
  'figure{margin:0}',
  'figcaption{font-size:0.78rem;line-height:1.3;color:#3b3f45}',
  'table{max-width:100%;border-collapse:collapse}',
  'td,th{border:1px solid #d8dde3;padding:0.32rem 0.45rem;vertical-align:top}',
  'table:not(.infobox):not(.vertical-navbox):not(.wikitable){display:block;overflow-x:auto}',
  '.thumb,.infobox,.gallery{max-width:100%}',
  '.thumb{margin:0.4rem 0 0.75rem}',
  '.thumb img{border:1px solid #c8ccd1;padding:2px;background:#ffffff}',
  '.infobox{float:right;margin:0 0 0.8rem 0.9rem;font-size:0.86rem;',
  'width:min(320px,100%);background:#f8f9fa}',
  '.infobox td,.infobox th{border-color:#c8ccd1}',
  'a.item{display:inline-flex;width:172px;max-width:100%;margin:0.32rem;',
  'border:1px solid #c8ccd1;border-radius:2px;overflow:hidden;',
  'vertical-align:top;color:#202122;background:#f8f9fa}',
  'a.item:hover{border-color:#a2a9b1;background:#f1f3f5}',
  'a.item figure{display:flex;flex-direction:column;width:100%}',
  'a.item img{width:100%;aspect-ratio:4/3;object-fit:cover}',
  'a.item figcaption{padding:0.42rem 0.5rem}'
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
    '  window.parent.postMessage({ type: "zim-height", height: height }, location.origin);' +
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
    '  window.parent.postMessage({ type: "zim-navigate", href: href }, location.origin);' +
    '}, true);' +
    'window.addEventListener("load", fyrNotifyHeight);' +
    'window.addEventListener("resize", fyrNotifyHeight);' +
    'new MutationObserver(function() { fyrNotifyHeight(); }).observe(document.documentElement, { childList: true, subtree: true, attributes: true, characterData: true });' +
    'setTimeout(fyrNotifyHeight, 0);' +
    'setTimeout(fyrNotifyHeight, 300);' +
    'setTimeout(fyrNotifyHeight, 1200);' +
    '</script>'

  const scrollOverride =
    '<style>' +
    'html,body{overflow:hidden!important;overscroll-behavior:contain}' +
    'body{max-width:100%;}' +
    '</style>'

  return (
    '<!DOCTYPE html><html><head>' +
    '<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">' +
    `<style>${ZIM_ARTICLE_BASE_CSS}</style>` +
    headHtml +
    scrollOverride +
    navScript +
    '</head><body>' +
    bodyHtml +
    '</body></html>'
  )
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

    const normalizedPath = decodePathDeep(resolved.pathname).replace(/^\/+/, '')
    if (!normalizedPath) {
      return `${resolved.search}${resolved.hash}`
    }
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

export const useZimReader = () => {
  const meta = ref(null)
  const adapter = ref(null)
  const nativeArticle = ref(null)

  const dispose = () => {
    meta.value = null
    adapter.value = null
    nativeArticle.value = null
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
    const native = await apiService.getZimNativeArticle(filename, path)
    const rendered = rewriteNativeZimHtml(
      filename,
      native?.path,
      native?.content,
      apiService
    )

    nativeArticle.value = {
      ...native,
      content: buildZimSandboxDocument(rendered.headHtml, sanitizeNativeZimBodyHtml(rendered.bodyHtml))
    }

    return nativeArticle.value
  }

  return {
    meta,
    adapter,
    nativeArticle,
    open,
    loadNativeArticle,
    dispose,
    decodePathDeep
  }
}
