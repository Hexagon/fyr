#!/usr/bin/env node
// Builds the generated parts of the fyr.guide static site (docs/site):
//
//   - css/site.css            prebuilt, minified Tailwind stylesheet (replaces the Play CDN)
//   - guides/**, compare/     pages rendered from docs/build/site/content/*.html
//   - docs/user-manual/,      rendered from the canonical docs/user/USER_MANUAL.md
//     docs/developer-manual/  and docs/developer/DEVELOPER_MANUAL.md
//   - guides/, docs/, 404.html, sitemap.xml
//   - index.html              only the marked build regions (JSON-LD, footer,
//                             GitHub CTA script, version label, CSS hash);
//                             everything else in index.html is hand-written.
//
// GitHub Pages deploys docs/site verbatim, so outputs are committed.
// `node build-site.js --check` rebuilds in memory and exits non-zero if any
// committed output is stale (used by CI).

const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { execFileSync } = require('child_process');
const { Marked } = require('marked');
const L = require('./site/layout');

const CHECK = process.argv.includes('--check');
const ROOT = path.resolve(__dirname, '..', '..');
const SITE = path.join(ROOT, 'docs', 'site');
const SRC = path.join(__dirname, 'site');
const CONTENT = path.join(SRC, 'content');
const MANIFEST = path.join(SRC, 'lastmod.json');
const { SITE_URL, REPO_URL, esc } = L;
const TODAY = new Date().toISOString().slice(0, 10);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
const sha = (s) => crypto.createHash('sha256').update(s).digest('hex');
const stripTags = (s) =>
  s.replace(/<[^>]+>/g, '')
    .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ').trim();

function slugify(text, used) {
  // GitHub-compatible heading slugs so existing #anchors in the manuals keep working.
  let base = text.toLowerCase().replace(/[^\p{L}\p{N}\s_-]/gu, '').replace(/\s/g, '-');
  let slug = base;
  let n = 1;
  while (used.has(slug)) slug = `${base}-${n++}`;
  used.add(slug);
  return slug;
}

function readVersion() {
  const toml = fs.readFileSync(path.join(ROOT, 'Cargo.toml'), 'utf8');
  const m = toml.match(/\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/m);
  if (!m) throw new Error('Could not read [workspace.package] version from Cargo.toml');
  return m[1];
}
const VERSION = readVersion();

// Adds ids + anchor links to h2/h3 in an HTML fragment and returns a TOC.
function anchorHeadings(html, { tocDepth = 3 } = {}) {
  const used = new Set();
  const toc = [];
  const out = html.replace(/<h([23])((?:\s[^>]*)?)>([\s\S]*?)<\/h\1>/g, (all, lvl, attrs, inner) => {
    const text = stripTags(inner);
    let id;
    const idm = attrs.match(/\sid="([^"]+)"/);
    if (idm) { id = idm[1]; used.add(id); } else { id = slugify(text, used); attrs = ` id="${id}"${attrs}`; }
    if (Number(lvl) <= tocDepth) toc.push({ depth: Number(lvl), id, text });
    return `<h${lvl}${attrs}>${inner}<a class="heading-anchor" href="#${id}" aria-label="Link to this section">#</a></h${lvl}>`;
  });
  return { html: out, toc };
}

const readMinutes = (html) => Math.max(1, Math.round(stripTags(html).split(' ').length / 220));

const ORG = { '@type': 'Organization', '@id': 'https://github.com/Hexagon#organization', name: 'Hexagon', url: 'https://github.com/Hexagon' };
const SOFTWARE_ID = `${SITE_URL}/#software`;

function breadcrumbLd(crumbs) {
  return {
    '@type': 'BreadcrumbList',
    itemListElement: crumbs.map((c, i) => ({ '@type': 'ListItem', position: i + 1, name: c.name, item: SITE_URL + c.path })),
  };
}

function articleLd({ type = 'TechArticle', path: p, h1, description, published }) {
  return {
    '@type': type,
    headline: h1,
    description,
    url: SITE_URL + p,
    mainEntityOfPage: SITE_URL + p,
    image: `${SITE_URL}/img/og-image.jpg`,
    inLanguage: 'en',
    datePublished: published,
    dateModified: '__LASTMOD__',
    author: { '@type': 'Organization', name: 'Fyr Contributors', url: REPO_URL },
    publisher: { '@id': ORG['@id'] },
    about: { '@id': SOFTWARE_ID },
  };
}

// ---------------------------------------------------------------------------
// Content pages (guides + compare)
// ---------------------------------------------------------------------------
function loadContent(file) {
  const raw = fs.readFileSync(file, 'utf8');
  const m = raw.match(/^<!--meta\s*([\s\S]*?)-->\s*/);
  if (!m) throw new Error(`${file}: missing <!--meta {...} --> header`);
  const meta = JSON.parse(m[1]);
  return { ...meta, body: raw.slice(m[0].length), file };
}

const guides = fs.readdirSync(path.join(CONTENT, 'guides')).filter((f) => f.endsWith('.html')).sort()
  .map((f) => loadContent(path.join(CONTENT, 'guides', f)))
  .sort((a, b) => (a.order || 99) - (b.order || 99));
const compare = loadContent(path.join(CONTENT, 'compare.html'));

const pages = []; // { rel, html, path, sitemap }

function relatedBlock(currentPath) {
  const items = [...guides, compare].filter((g) => g.path !== currentPath);
  return `
<h2 id="related">Related guides</h2>
<ul>
${items.map((g) => `<li><a href="${g.path}">${esc(g.h1)}</a> — ${esc(g.description)}</li>`).join('\n')}
</ul>`;
}

function renderContentPage(c, crumbs) {
  const { html: body0, toc } = anchorHeadings(c.body + relatedBlock(c.path), { tocDepth: 2 });
  const jsonld = [articleLd({ type: c.schemaType || 'TechArticle', path: c.path, h1: c.h1, description: c.description, published: c.published }), breadcrumbLd(crumbs)];
  // HowTo steps: <h2 data-step ...>Name</h2> followed by a <p>.
  const steps = [];
  body0.replace(/<h2 id="([^"]+)"[^>]*\sdata-step[^>]*>([\s\S]*?)<\/h2>\s*<p>([\s\S]*?)<\/p>/g, (a, id, name, text) => {
    steps.push({ '@type': 'HowToStep', name: stripTags(name).replace(/#$/, ''), text: stripTags(text), url: `${SITE_URL}${c.path}#${id}` });
  });
  if (steps.length) {
    jsonld.push({ '@type': 'HowTo', name: c.h1, description: c.description, step: steps, ...(c.totalTime ? { totalTime: c.totalTime } : {}) });
  }
  return L.page({
    path: c.path,
    title: c.title,
    description: c.description,
    h1: c.h1,
    lead: c.lead,
    crumbs,
    jsonld,
    body: body0,
    toc,
    active: c.active,
    meta: `Updated <time datetime="__LASTMOD__">__LASTMOD__</time> · ${readMinutes(body0)} min read · Applies to Fyr ${esc('v' + VERSION)}`,
  });
}

for (const g of guides) {
  g.active = 'guides';
  const crumbs = [{ name: 'Home', path: '/' }, { name: 'Guides', path: '/guides/' }, { name: g.crumb || g.h1, path: g.path }];
  pages.push({ path: g.path, html: renderContentPage(g, crumbs), sitemap: true });
}
compare.active = 'compare';
pages.push({ path: compare.path, html: renderContentPage(compare, [{ name: 'Home', path: '/' }, { name: compare.crumb, path: compare.path }]), sitemap: true });

// Guides index
{
  const p = '/guides/';
  const crumbs = [{ name: 'Home', path: '/' }, { name: 'Guides', path: p }];
  const cards = [...guides, compare].map((g) => `<a href="${g.path}" class="not-prose block p-6 bg-gray-100 rounded-2xl border border-gray-300 hover:border-brand-500 hover:shadow-md transition-all group">
    <h2 class="text-xl font-semibold text-gray-900 group-hover:text-brand-600 transition-colors">${esc(g.h1)}</h2>
    <p class="mt-2 text-gray-600">${esc(g.description)}</p>
    <span class="mt-3 inline-block text-brand-600 font-medium">Read the guide →</span>
</a>`).join('\n');
  pages.push({
    path: p,
    sitemap: true,
    html: L.page({
      path: p,
      title: 'Fyr Guides: Offline Wikipedia, Maps, Local AI and Off-Grid Servers',
      description: 'Step-by-step guides for running Fyr: offline Wikipedia on a Raspberry Pi, a self-hosted offline maps server, a CPU-only local AI assistant, and an off-grid emergency knowledge hotspot.',
      h1: 'Guides',
      lead: 'Practical, step-by-step setups for the most common things people build with Fyr.',
      crumbs,
      active: 'guides',
      jsonld: [{ '@type': 'CollectionPage', name: 'Fyr Guides', url: SITE_URL + p, hasPart: guides.map((g) => ({ '@type': 'TechArticle', headline: g.h1, url: SITE_URL + g.path })) }, breadcrumbLd(crumbs)],
      body: `<div class="not-prose grid grid-cols-1 md:grid-cols-2 gap-6">\n${cards}\n</div>`,
    }),
  });
}

// ---------------------------------------------------------------------------
// Manuals (rendered from canonical markdown)
// ---------------------------------------------------------------------------
const MANUALS = [
  {
    src: 'docs/user/USER_MANUAL.md',
    path: '/docs/user-manual/',
    crumb: 'User Manual',
    title: 'Fyr User Manual: Setup, Maps, ZIM, Local AI and Troubleshooting',
    description: 'The Fyr User Manual: access control, the offline AI assistant and model tiers, adding maps, books, ZIM archives and POIs, data layout, platform notes and troubleshooting.',
    lead: 'Operator and end-user workflows for Fyr — generated from the canonical <code class="text-brand-400">docs/user/USER_MANUAL.md</code>.',
  },
  {
    src: 'docs/developer/DEVELOPER_MANUAL.md',
    path: '/docs/developer-manual/',
    crumb: 'Developer Manual',
    title: 'Fyr Developer Manual: Architecture, API, AI Runtime and Releases',
    description: 'The Fyr Developer Manual: Rust workspace layout, Candle GGUF inference, access-control architecture, API endpoints, Docker multi-arch builds and the release process.',
    lead: 'Implementation and architecture notes for contributors — generated from the canonical <code class="text-brand-400">docs/developer/DEVELOPER_MANUAL.md</code>.',
  },
];

function rewriteManualHref(href, srcRel) {
  if (/^https:\/\/fyr\.guide(\/|$)/.test(href)) return href.replace(/^https:\/\/fyr\.guide/, '') || '/';
  if (/^[a-z]+:/i.test(href) || href.startsWith('#')) return href;
  const [p, hash] = href.split('#');
  const resolved = p.startsWith('/') ? p.slice(1) : path.posix.normalize(path.posix.join(path.posix.dirname(srcRel), p));
  const frag = hash ? `#${hash}` : '';
  if (resolved === 'docs/user/USER_MANUAL.md') return `/docs/user-manual/${frag}`;
  if (resolved === 'docs/developer/DEVELOPER_MANUAL.md') return `/docs/developer-manual/${frag}`;
  if (resolved === 'docs/site/index.html') return `/${frag}`;
  return `${REPO_URL}/blob/main/${resolved}${frag}`;
}

for (const man of MANUALS) {
  // GFM splits table cells on every unescaped "|", even inside `code` spans
  // (e.g. "`curl … | sh`"). Escape those pipes so the one-liners render intact.
  const md = fs.readFileSync(path.join(ROOT, man.src), 'utf8')
    .split('\n')
    .map((line) => (/^\s*\|/.test(line) ? line.replace(/`[^`]*`/g, (span) => span.replace(/(?<!\\)\|/g, '\\|')) : line))
    .join('\n');
  const marked = new Marked({ gfm: true });
  marked.use({
    walkTokens(token) {
      if (token.type === 'link') token.href = rewriteManualHref(token.href, man.src);
    },
    renderer: {
      heading({ tokens, depth }) {
        if (depth === 1) return ''; // page <h1> comes from the layout
        return `<h${depth}>${this.parser.parseInline(tokens)}</h${depth}>\n`;
      },
      table(token) {
        return `<div class="overflow-x-auto">${this.constructor.prototype.table.call(this, token)}</div>\n`;
      },
      link({ href, title, tokens }) {
        const text = this.parser.parseInline(tokens);
        const ext = /^https?:/.test(href) && !href.startsWith(SITE_URL);
        return `<a href="${esc(href)}"${title ? ` title="${esc(title)}"` : ''}${ext ? ' rel="noopener"' : ''}>${text}</a>`;
      },
    },
  });
  const rendered = marked.parse(md);
  const { html, toc } = anchorHeadings(rendered, { tocDepth: 3 });
  const crumbs = [{ name: 'Home', path: '/' }, { name: 'Documentation', path: '/docs/' }, { name: man.crumb, path: man.path }];
  const notice = `<p class="not-prose text-sm text-gray-600 bg-gray-200/70 border border-gray-300 rounded-lg px-4 py-3 mb-8">This page is generated from <a class="text-brand-600 underline" href="${REPO_URL}/blob/main/${man.src}">${man.src}</a> on GitHub, the canonical source. Installation steps live on the <a class="text-brand-600 underline" href="/#installation">installation section</a>.</p>`;
  pages.push({
    path: man.path,
    sitemap: true,
    html: L.page({
      path: man.path,
      title: man.title,
      description: man.description,
      h1: man.crumb,
      lead: man.lead,
      crumbs,
      active: 'docs',
      wide: true,
      toc,
      meta: `Updated <time datetime="__LASTMOD__">__LASTMOD__</time> · Fyr ${esc('v' + VERSION)}`,
      jsonld: [articleLd({ path: man.path, h1: `Fyr ${man.crumb}`, description: man.description, published: '2026-01-01' }), breadcrumbLd(crumbs)],
      body: notice + html,
    }),
  });
}

// Docs index
{
  const p = '/docs/';
  const crumbs = [{ name: 'Home', path: '/' }, { name: 'Documentation', path: p }];
  const card = (href, h, d, ext) => `<a href="${href}"${ext ? ' rel="noopener"' : ''} class="block p-8 bg-gray-100 border border-gray-300 rounded-2xl hover:border-brand-500 hover:shadow-md transition-all group">
    <h2 class="text-xl font-bold text-gray-900 group-hover:text-brand-600 transition-colors">${h}</h2>
    <p class="mt-2 text-gray-600 text-sm">${d}</p>
</a>`;
  pages.push({
    path: p,
    sitemap: true,
    html: L.page({
      path: p,
      title: 'Fyr Documentation: User Manual, Developer Manual and Guides',
      description: 'Fyr documentation hub: the User Manual for operators, the Developer Manual for contributors, step-by-step guides, and the installation guide.',
      h1: 'Documentation',
      lead: 'Fyr is in preview, so check the docs with each upgrade. The manuals below are rendered from the canonical Markdown in the repository.',
      crumbs,
      active: 'docs',
      jsonld: [{ '@type': 'CollectionPage', name: 'Fyr Documentation', url: SITE_URL + p }, breadcrumbLd(crumbs)],
      body: `<div class="not-prose grid grid-cols-1 md:grid-cols-2 gap-6">
${card('/docs/user-manual/', 'User Manual', 'Access control, AI assistant setup and model tiers, adding content, data directories, and troubleshooting.')}
${card('/docs/developer-manual/', 'Developer Manual', 'Architecture, API endpoints, Candle inference runtime, Docker builds, and the release process.')}
${card('/#installation', 'Installation', 'Hosted installer scripts, manual Docker run, Raspberry Pi hotspot setup, and building from source.')}
${card('/guides/', 'Guides', 'Offline Wikipedia, offline maps, local AI, and off-grid emergency knowledge server walkthroughs.')}
${card(`${REPO_URL}/blob/main/README.md`, 'README (GitHub)', 'Project overview, quick start, and a navigation hub for all canonical docs.', true)}
${card(`${REPO_URL}/blob/main/CONTRIBUTING.md`, 'Contributing (GitHub)', 'Branching strategy, validation checklist, and documentation update requirements.', true)}
</div>`,
    }),
  });
}

// 404 (GitHub Pages serves /404.html for any missing path)
pages.push({
  path: '/404.html',
  sitemap: false,
  html: L.page({
    path: '/404.html',
    title: 'Page not found | Fyr',
    description: 'This page does not exist on fyr.guide.',
    robots: 'noindex',
    h1: 'Page not found',
    lead: 'The page you were looking for does not exist or has moved.',
    body: `<p>Try one of these instead:</p>
<ul>
<li><a href="/">Fyr home</a> — what Fyr is and how it works</li>
<li><a href="/#installation">Install Fyr</a> — one-line installer, Docker, or build from source</li>
<li><a href="/guides/">Guides</a> — offline Wikipedia, offline maps, local AI, off-grid servers</li>
<li><a href="/docs/">Documentation</a> — User and Developer manuals</li>
</ul>`,
  }),
});

// ---------------------------------------------------------------------------
// Landing page: only replace the marked build regions
// ---------------------------------------------------------------------------
function region(html, name, content) {
  const re = new RegExp(`(<!-- build:${name} -->)[\\s\\S]*?(<!-- /build:${name} -->)`);
  if (!re.test(html)) throw new Error(`index.html: missing <!-- build:${name} --> region`);
  return html.replace(re, `$1\n${content}\n    $2`);
}

function landingJsonLd(indexHtml) {
  const faqSection = indexHtml.match(/<div id="faq"[\s\S]*?<!-- \/faq -->/);
  if (!faqSection) throw new Error('index.html: could not find FAQ section (<div id="faq"> … <!-- /faq -->)');
  const faqs = [];
  faqSection[0].replace(/<h3[^>]*>([\s\S]*?)<\/h3>\s*<p[^>]*>([\s\S]*?)<\/p>/g, (a, q, ans) => {
    faqs.push({ '@type': 'Question', name: stripTags(q), acceptedAnswer: { '@type': 'Answer', text: stripTags(ans) } });
  });
  if (faqs.length < 3) throw new Error('index.html: FAQ extraction found too few questions');
  const desc = 'Fyr is a self-hosted offline knowledge server for maps, books, and local AI. Runs on a Raspberry Pi, homelab, or any Linux box — no internet required once content is loaded.';
  const graph = [
    {
      '@type': 'SoftwareApplication',
      '@id': SOFTWARE_ID,
      name: 'Fyr',
      url: `${SITE_URL}/`,
      description: desc,
      applicationCategory: 'UtilitiesApplication',
      applicationSubCategory: 'Offline knowledge server',
      operatingSystem: 'Linux, macOS, Windows (Docker); Raspberry Pi OS 64-bit',
      softwareVersion: VERSION,
      releaseNotes: `${REPO_URL}/releases`,
      downloadUrl: 'https://hub.docker.com/r/hexagon/fyr',
      installUrl: `${SITE_URL}/#installation`,
      screenshot: [1, 2, 3, 4].map((n) => `${SITE_URL}/img/screenshot-${n}.png`),
      image: `${SITE_URL}/img/og-image.jpg`,
      featureList: [
        'Offline vector and raster maps from PMTiles and MBTiles',
        'Reader for EPUB, PDF, Markdown and Kiwix ZIM archives (e.g. offline Wikipedia)',
        'Local CPU-only AI assistant for GGUF models (Qwen2, Llama, Phi-3)',
        'Download queue and content manager',
        'Password-protected admin mode and strict read-only kiosk mode',
        'Offline unit converters and encryption tools',
      ],
      offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
      isAccessibleForFree: true,
      license: `${REPO_URL}/blob/main/LICENSE`,
      codeRepository: REPO_URL,
      publisher: { '@id': ORG['@id'] },
    },
    { ...ORG, sameAs: [REPO_URL] },
    { '@type': 'WebSite', '@id': `${SITE_URL}/#website`, name: 'Fyr Guide', url: `${SITE_URL}/`, publisher: { '@id': ORG['@id'] } },
    { '@type': 'FAQPage', '@id': `${SITE_URL}/#faq`, mainEntity: faqs },
  ];
  return `    <script type="application/ld+json">\n${JSON.stringify({ '@context': 'https://schema.org', '@graph': graph }, null, 2)}\n    </script>`;
}

let index = fs.readFileSync(path.join(SITE, 'index.html'), 'utf8');
index = region(index, 'jsonld', landingJsonLd(index));
index = region(index, 'cta', L.ctaBand());
index = region(index, 'footer', L.FOOTER);
index = region(index, 'gh-script', L.GH_SCRIPT);
pages.unshift({ path: '/', html: index, sitemap: true, file: 'index.html' });

// Version label + CSS cache-buster placeholders on every page
for (const pg of pages) {
  pg.html = pg.html
    .replace(/(<[^>]*\bdata-gh-release\b[^>]*>)[^<]*(<)/g, `$1v${VERSION}$2`)
    .replace(/__VERSION_TAG__/g, `v${VERSION}`)
    .replace(/\/css\/site\.css\?v=[A-Za-z0-9_]*/g, '/css/site.css?v=__CSS_HASH__');
  pg.file = pg.file || (pg.path.endsWith('/') ? pg.path.slice(1) + 'index.html' : pg.path.slice(1));
}

// ---------------------------------------------------------------------------
// lastmod tracking (content hash -> date), so sitemap dates are real and stable
// ---------------------------------------------------------------------------
const manifest = fs.existsSync(MANIFEST) ? JSON.parse(fs.readFileSync(MANIFEST, 'utf8')) : {};
const newManifest = {};
for (const pg of pages) {
  const h = sha(pg.html).slice(0, 16);
  const prev = manifest[pg.path];
  pg.lastmod = prev && prev.hash === h ? prev.lastmod : TODAY;
  newManifest[pg.path] = { hash: h, lastmod: pg.lastmod };
  pg.html = pg.html.replace(/__LASTMOD__/g, pg.lastmod);
}

// ---------------------------------------------------------------------------
// Tailwind CSS (scans the final HTML of every page)
// ---------------------------------------------------------------------------
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'fyr-site-'));
let css;
try {
  for (const pg of pages) {
    const f = path.join(tmp, 'pages', pg.file);
    fs.mkdirSync(path.dirname(f), { recursive: true });
    fs.writeFileSync(f, pg.html);
  }
  const cfg = path.join(tmp, 'tailwind.config.js');
  fs.writeFileSync(cfg, `const base = require(${JSON.stringify(path.join(SRC, 'tailwind.config.js'))});
module.exports = { ...base, content: [${JSON.stringify(path.join(tmp, 'pages').replace(/\\/g, '/') + '/**/*.html')}, ${JSON.stringify(path.join(SRC, 'layout.js').replace(/\\/g, '/'))}] };\n`);
  const outCss = path.join(tmp, 'site.css');
  execFileSync(process.execPath, [require.resolve('tailwindcss/lib/cli.js'), '-c', cfg, '-i', path.join(SRC, 'input.css'), '-o', outCss, '--minify'], { stdio: ['ignore', 'ignore', 'pipe'] });
  css = fs.readFileSync(outCss, 'utf8');
} finally {
  fs.rmSync(tmp, { recursive: true, force: true });
}
const cssHash = sha(css).slice(0, 10);
for (const pg of pages) pg.html = pg.html.replace(/__CSS_HASH__/g, cssHash);

// ---------------------------------------------------------------------------
// sitemap.xml
// ---------------------------------------------------------------------------
const sitemap = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${pages.filter((p) => p.sitemap).map((p) => `  <url>
    <loc>${SITE_URL}${p.path}</loc>
    <lastmod>${p.lastmod}</lastmod>
  </url>`).join('\n')}
</urlset>
`;

// ---------------------------------------------------------------------------
// Write or check
// ---------------------------------------------------------------------------
const outputs = new Map();
for (const pg of pages) outputs.set(path.join(SITE, pg.file), pg.html);
outputs.set(path.join(SITE, 'css', 'site.css'), css);
outputs.set(path.join(SITE, 'sitemap.xml'), sitemap);
outputs.set(MANIFEST, JSON.stringify(newManifest, null, 2) + '\n');

// Generated directories: any index.html there that we did not generate is an orphan.
const GENERATED_DIRS = ['guides', 'compare', 'docs'].map((d) => path.join(SITE, d));
const orphans = [];
for (const dir of GENERATED_DIRS) {
  if (!fs.existsSync(dir)) continue;
  const walk = (d) => fs.readdirSync(d, { withFileTypes: true }).forEach((e) => {
    const f = path.join(d, e.name);
    if (e.isDirectory()) walk(f);
    else if (!outputs.has(f)) orphans.push(f);
  });
  walk(dir);
}

if (CHECK) {
  const stale = [];
  for (const [f, content] of outputs) {
    if (!fs.existsSync(f) || fs.readFileSync(f, 'utf8') !== content) stale.push(path.relative(ROOT, f));
  }
  stale.push(...orphans.map((f) => `${path.relative(ROOT, f)} (orphan)`));
  if (stale.length) {
    console.error('Generated site files are stale:\n  ' + stale.join('\n  '));
    console.error('\nRun `cd docs/build && npm run build` and commit the result.');
    process.exit(1);
  }
  console.log(`Site is up to date (${outputs.size} generated files, Fyr v${VERSION}).`);
} else {
  for (const [f, content] of outputs) {
    fs.mkdirSync(path.dirname(f), { recursive: true });
    if (!fs.existsSync(f) || fs.readFileSync(f, 'utf8') !== content) {
      fs.writeFileSync(f, content);
      console.log(`wrote ${path.relative(ROOT, f)}`);
    }
  }
  for (const f of orphans) {
    fs.rmSync(f);
    console.log(`removed orphan ${path.relative(ROOT, f)}`);
  }
  console.log(`Site build complete: ${pages.length} pages, css ${(css.length / 1024).toFixed(1)} KB, Fyr v${VERSION}.`);
}
