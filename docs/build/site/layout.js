// Shared page chrome (head, nav, footer) for the generated fyr.guide pages.
// The landing page (docs/site/index.html) is hand-maintained; build-site.js
// injects FOOTER and the GitHub CTA script into it between build markers so
// every page shares the same footer.

const SITE_URL = 'https://fyr.guide';
const REPO_URL = 'https://github.com/Hexagon/fyr';
const CSS_HREF = '/css/site.css?v=__CSS_HASH__';

const esc = (s) =>
  String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

const GITHUB_ICON =
  '<svg class="h-6 w-6" fill="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path fill-rule="evenodd" d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z" clip-rule="evenodd" /></svg>';

function head({ path, title, description, ogType = 'article', robots = 'index,follow,max-image-preview:large', jsonld = [] }) {
  const url = SITE_URL + path;
  const ld = jsonld.length
    ? `\n    <script type="application/ld+json">\n${JSON.stringify({ '@context': 'https://schema.org', '@graph': jsonld }, null, 2)}\n    </script>`
    : '';
  const canonical = robots.startsWith('noindex') ? '' : `\n    <link rel="canonical" href="${url}">`;
  return `<!DOCTYPE html>
<html lang="en" class="scroll-smooth">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>${esc(title)}</title>
    <meta name="description" content="${esc(description)}">
    <meta name="robots" content="${robots}">${canonical}
    <meta name="theme-color" content="#ea580c">
    <link rel="icon" type="image/svg+xml" href="/img/favicon.svg">
    <link rel="icon" type="image/png" sizes="32x32" href="/img/favicon-32.png">
    <link rel="icon" type="image/x-icon" href="/img/favicon.ico">
    <link rel="apple-touch-icon" sizes="180x180" href="/img/apple-touch-icon.png">
    <link rel="manifest" href="/site.webmanifest">
    <link rel="preload" href="/fonts/inter-latin.woff2" as="font" type="font/woff2" crossorigin>
    <link rel="stylesheet" href="${CSS_HREF}">

    <meta property="og:type" content="${ogType}">
    <meta property="og:url" content="${url}">
    <meta property="og:title" content="${esc(title)}">
    <meta property="og:description" content="${esc(description)}">
    <meta property="og:image" content="${SITE_URL}/img/og-image.jpg">
    <meta property="og:image:width" content="1200">
    <meta property="og:image:height" content="630">
    <meta property="og:image:alt" content="Fyr web interface showing an offline map with layers and a map picker">
    <meta property="og:site_name" content="Fyr">
    <meta name="twitter:card" content="summary_large_image">
    <meta name="twitter:title" content="${esc(title)}">
    <meta name="twitter:description" content="${esc(description)}">
    <meta name="twitter:image" content="${SITE_URL}/img/og-image.jpg">
    <meta name="twitter:image:alt" content="Fyr web interface showing an offline map with layers and a map picker">${ld}
</head>`;
}

function nav(active) {
  const item = (href, label, key) => {
    const cur = active === key;
    return `<a href="${href}"${cur ? ' aria-current="page"' : ''} class="${cur ? 'text-white' : 'text-gray-300'} hover:text-brand-200 font-medium transition-colors">${label}</a>`;
  };
  return `    <nav class="fixed w-full bg-gray-950/85 backdrop-blur-md border-b border-white/10 z-50" aria-label="Main">
        <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
            <div class="flex justify-between h-16 items-center">
                <a href="/" class="flex-shrink-0 flex items-center gap-2" aria-label="Fyr home">
                    <div class="w-8 h-8 rounded bg-gradient-to-br from-brand-500 to-brand-600 text-white flex items-center justify-center font-bold text-xl" aria-hidden="true">F</div>
                    <span class="font-bold text-xl tracking-tight text-white">Fyr<span class="text-brand-300">.guide</span></span>
                </a>
                <div class="hidden md:flex space-x-8">
                    ${item('/#features', 'Features', 'features')}
                    ${item('/guides/', 'Guides', 'guides')}
                    ${item('/compare/', 'Compare', 'compare')}
                    ${item('/#installation', 'Installation', 'install')}
                    ${item('/docs/', 'Documentation', 'docs')}
                </div>
                <div>
                    <a href="${REPO_URL}" target="_blank" rel="noopener" class="text-gray-400 hover:text-white transition-colors" aria-label="Fyr on GitHub">
                        ${GITHUB_ICON}
                    </a>
                </div>
            </div>
        </div>
    </nav>`;
}

// Shared footer: also injected into index.html between <!-- build:footer --> markers.
const FOOTER = `    <footer class="bg-gray-200 py-12">
        <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 flex flex-col md:flex-row justify-between items-center gap-6">
            <div class="flex items-center gap-2">
                <div class="w-6 h-6 rounded bg-gray-900 text-white flex items-center justify-center font-bold text-xs" aria-hidden="true">F</div>
                <span class="font-semibold text-gray-900">Fyr Content Platform</span>
            </div>
            <div class="text-sm text-gray-500">
                Released under the MIT License.
            </div>
            <div class="flex space-x-6">
                <a href="${REPO_URL}" class="text-gray-400 hover:text-gray-900 transition-colors">
                    <span class="sr-only">GitHub</span>
                    ${GITHUB_ICON}
                </a>
            </div>
        </div>
        <nav class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 mt-8 flex flex-wrap justify-center gap-x-6 gap-y-2 text-sm text-gray-500" aria-label="Footer">
            <a href="/guides/offline-wikipedia-raspberry-pi/" class="hover:text-gray-900 transition-colors">Offline Wikipedia</a>
            <a href="/guides/offline-maps-server/" class="hover:text-gray-900 transition-colors">Offline maps</a>
            <a href="/guides/local-ai-assistant-offline/" class="hover:text-gray-900 transition-colors">Local AI</a>
            <a href="/guides/off-grid-emergency-knowledge-server/" class="hover:text-gray-900 transition-colors">Off-grid server</a>
            <a href="/compare/" class="hover:text-gray-900 transition-colors">Compare</a>
            <a href="/docs/user-manual/" class="hover:text-gray-900 transition-colors">User Manual</a>
            <a href="/docs/developer-manual/" class="hover:text-gray-900 transition-colors">Developer Manual</a>
            <a href="${REPO_URL}/releases" class="hover:text-gray-900 transition-colors">Releases</a>
        </nav>
    </footer>`;

// Progressive enhancement for elements marked data-gh-stars / data-gh-release.
// Static text (version from Cargo.toml, plain "Star on GitHub") is the offline
// fallback; the GitHub API result is cached per session to stay under limits.
const GH_SCRIPT = `    <script>
    (function () {
        var stars = document.querySelectorAll('[data-gh-stars]');
        var rels = document.querySelectorAll('[data-gh-release]');
        if ((!stars.length && !rels.length) || !window.fetch) return;
        function apply(d) {
            if (!d) return;
            if (typeof d.stars === 'number') stars.forEach(function (el) { el.textContent = d.stars.toLocaleString('en-US'); el.hidden = false; });
            if (d.tag) rels.forEach(function (el) { el.textContent = d.tag; });
        }
        var KEY = 'fyr-gh-meta';
        try { var c = JSON.parse(sessionStorage.getItem(KEY) || 'null'); if (c) { apply(c); return; } } catch (e) {}
        function get(u) { return fetch(u, { headers: { Accept: 'application/vnd.github+json' } }).then(function (r) { return r.ok ? r.json() : null; }).catch(function () { return null; }); }
        Promise.all([get('https://api.github.com/repos/Hexagon/fyr'), get('https://api.github.com/repos/Hexagon/fyr/releases/latest')]).then(function (res) {
            var d = { stars: res[0] && res[0].stargazers_count, tag: res[1] && res[1].tag_name };
            apply(d);
            try { if (d.stars != null || d.tag) sessionStorage.setItem(KEY, JSON.stringify(d)); } catch (e) {}
        });
    })();
    </script>`;

function breadcrumbsHtml(crumbs) {
  return `<nav aria-label="Breadcrumb" class="text-sm text-gray-400 mb-4"><ol class="flex flex-wrap items-center gap-x-2">${crumbs
    .map((c, i) =>
      i === crumbs.length - 1
        ? `<li aria-current="page" class="text-gray-300">${esc(c.name)}</li>`
        : `<li><a href="${c.path}" class="hover:text-brand-200 transition-colors">${esc(c.name)}</a></li><li aria-hidden="true">/</li>`
    )
    .join('')}</ol></nav>`;
}

function ctaBand() {
  return `    <section class="py-16 bg-gray-900 text-gray-300 border-t border-gray-800" aria-labelledby="cta-heading">
        <div class="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8 text-center">
            <h2 id="cta-heading" class="text-3xl font-bold text-white mb-4">Run Fyr on your own hardware</h2>
            <p class="text-lg text-gray-400 mb-8">Free and open source (MIT). One container or one binary, on a Raspberry Pi, homelab, or any Linux, macOS, or Windows machine with Docker.</p>
            <div class="flex flex-col sm:flex-row justify-center gap-4">
                <a href="/#installation" class="w-full sm:w-auto text-center bg-brand-500 hover:bg-brand-400 text-white px-8 py-3 rounded-lg font-medium transition-all shadow-lg shadow-brand-900/50">Install Fyr</a>
                <a href="${REPO_URL}" target="_blank" rel="noopener" class="w-full sm:w-auto inline-flex items-center justify-center gap-2 bg-white/10 border border-white/30 hover:bg-white/20 text-white px-8 py-3 rounded-lg font-medium transition-all">
                    <span aria-hidden="true">&#9733;</span> Star on GitHub <span data-gh-stars hidden class="text-xs bg-white/15 rounded px-2 py-0.5"></span>
                </a>
            </div>
            <p class="mt-6 text-sm text-gray-500">Latest release: <a href="${REPO_URL}/releases/latest" class="text-brand-400 hover:text-brand-200 underline" data-gh-release>__VERSION_TAG__</a></p>
        </div>
    </section>`;
}

// Generic page shell for guides, compare, docs and 404.
function page({ path, title, description, h1, lead, crumbs, jsonld, body, toc, meta, active, ogType, robots, wide }) {
  const tocList = (toc || [])
    .map((t) => `                            <li class="${t.depth === 3 ? 'pl-4' : ''}"><a href="#${t.id}" class="text-gray-600 hover:text-brand-600">${esc(t.text)}</a></li>`)
    .join('\n');
  // Mobile: collapsed <details> above the article. Desktop: sticky sidebar.
  const tocHtml = toc && toc.length
    ? `<details class="lg:hidden mb-8 bg-gray-200/70 border border-gray-300 rounded-xl p-4 text-sm">
                    <summary class="font-semibold text-gray-900 cursor-pointer">On this page</summary>
                    <ol class="mt-3 space-y-1.5">
${tocList}
                    </ol>
                </details>`
    : '';
  const tocAside = toc && toc.length
    ? `<aside class="hidden lg:block lg:col-span-3" aria-label="Table of contents">
                    <div class="sticky top-24 bg-gray-200/70 border border-gray-300 rounded-xl p-4 text-sm">
                        <p class="font-semibold text-gray-900">On this page</p>
                        <ol class="mt-3 space-y-1.5 max-h-[70vh] overflow-y-auto">
${tocList}
                        </ol>
                    </div>
                </aside>`
    : '';
  const articleCls = 'prose prose-gray max-w-none doc-content prose-headings:scroll-mt-20 prose-a:text-brand-600 prose-code:before:content-none prose-code:after:content-none prose-pre:bg-gray-900 prose-th:text-left prose-img:rounded-xl';
  return `${head({ path, title, description, ogType, robots, jsonld })}
<body class="bg-gray-200 text-gray-900 font-sans antialiased selection:bg-brand-500 selection:text-white">

${nav(active)}

    <header class="pt-28 pb-12 lg:pt-32 bg-gray-900 border-b border-gray-800">
        <div class="${wide ? 'max-w-7xl' : 'max-w-5xl'} mx-auto px-4 sm:px-6 lg:px-8">
            ${crumbs ? breadcrumbsHtml(crumbs) : ''}
            <h1 class="text-3xl sm:text-4xl md:text-5xl font-extrabold tracking-tight text-white">${esc(h1)}</h1>
            ${lead ? `<p class="mt-4 text-lg sm:text-xl text-gray-300 max-w-3xl [overflow-wrap:anywhere]">${lead}</p>` : ''}
            ${meta ? `<p class="mt-4 text-sm text-gray-400">${meta}</p>` : ''}
        </div>
    </header>

    <main class="py-12 bg-gray-100">
        <div class="${wide ? 'max-w-7xl' : 'max-w-5xl'} mx-auto px-4 sm:px-6 lg:px-8">
            ${tocHtml}
            <div class="${tocAside ? 'lg:grid lg:grid-cols-12 lg:gap-10' : ''}">
                <article class="${tocAside ? 'lg:col-span-9 min-w-0 ' : ''}${articleCls}">
${body}
                </article>
                ${tocAside}
            </div>
        </div>
    </main>

${ctaBand()}

${FOOTER}

${GH_SCRIPT}
</body>
</html>
`;
}

module.exports = { SITE_URL, REPO_URL, CSS_HREF, esc, head, nav, page, FOOTER, GH_SCRIPT, ctaBand, breadcrumbsHtml };
