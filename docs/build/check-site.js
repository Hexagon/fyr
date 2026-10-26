#!/usr/bin/env node
// Static checks for the fyr.guide site in docs/site (run in CI via `npm run check`):
//   - every page: <html lang>, exactly one <h1>, no skipped heading levels,
//     unique <title> and meta description, canonical URL, Open Graph/Twitter tags
//   - JSON-LD blocks parse and every node has an @type
//   - internal links/assets (href, src, srcset) resolve to files, and #fragments exist
//   - images have alt attributes
//   - code blocks marked data-sync="index" are identical to a block on the landing
//     page, so install snippets in guides cannot drift from the canonical ones
//   - sitemap.xml lists exactly the indexable pages; CNAME/robots/installers present
//   - no runtime CDN / third-party font dependencies

const fs = require('fs');
const path = require('path');

const SITE = path.resolve(__dirname, '..', 'site');
const SITE_URL = 'https://fyr.guide';
const errors = [];
const err = (file, msg) => errors.push(`${file}: ${msg}`);

function walk(dir, out = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const f = path.join(dir, e.name);
    if (e.isDirectory()) walk(f, out);
    else out.push(f);
  }
  return out;
}

const decode = (s) => s.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&nbsp;/g, ' ').replace(/&amp;/g, '&');
const text = (s) => decode(s.replace(/<[^>]+>/g, '')).replace(/\s+/g, ' ').trim();
const attr = (tag, name) => {
  const m = tag.match(new RegExp(`\\s${name}\\s*=\\s*("([^"]*)"|'([^']*)')`, 'i'));
  return m ? decode(m[2] !== undefined ? m[2] : m[3]) : null;
};
const urlPathOf = (file) => {
  const rel = '/' + path.relative(SITE, file).split(path.sep).join('/');
  return rel.endsWith('/index.html') ? rel.slice(0, -'index.html'.length) : rel;
};
// Strip <script> and <code>/<pre> bodies before scanning for markup-level checks.
const stripScripts = (html) => html.replace(/<script\b[\s\S]*?<\/script>/gi, '');

const files = walk(SITE);
const htmlFiles = files.filter((f) => f.endsWith('.html'));
const pages = new Map(); // file -> { html, ids }

for (const f of htmlFiles) {
  const html = fs.readFileSync(f, 'utf8');
  const ids = new Set([...stripScripts(html).matchAll(/\s(?:id|name)="([^"]+)"/g)].map((m) => m[1]));
  pages.set(f, { html, ids });
}

function resolveTarget(fromFile, ref) {
  // returns { file, frag } or null for external refs
  let u = ref.trim();
  if (!u || /^(mailto:|tel:|javascript:|data:)/i.test(u)) return null;
  if (u.startsWith(SITE_URL)) u = u.slice(SITE_URL.length) || '/';
  else if (/^[a-z][a-z0-9+.-]*:/i.test(u) || u.startsWith('//')) return null;
  const [pathPart, frag] = u.split('#');
  const clean = pathPart.split('?')[0];
  let target;
  if (clean === '') target = fromFile;
  else if (clean.startsWith('/')) target = path.join(SITE, decodeURIComponent(clean));
  else target = path.join(path.dirname(fromFile), decodeURIComponent(clean));
  if (fs.existsSync(target) && fs.statSync(target).isDirectory()) {
    if (clean && !clean.endsWith('/')) return { file: target, frag, error: 'directory link without trailing slash (redirect hop)' };
    target = path.join(target, 'index.html');
  }
  return { file: target, frag };
}

const titles = new Map();
const descriptions = new Map();
const indexable = new Set();
const indexHtml = pages.get(path.join(SITE, 'index.html'))?.html || '';
const indexCodeBlocks = new Set([...indexHtml.matchAll(/<pre\b[^>]*>([\s\S]*?)<\/pre>/g)].map((m) => decode(m[1].replace(/<[^>]+>/g, '')).trim()));

for (const [f, { html, ids }] of pages) {
  const rel = path.relative(SITE, f);
  const body = stripScripts(html);
  const noindex = /<meta name="robots" content="noindex/i.test(html);

  if (!/^<!DOCTYPE html>/i.test(html)) err(rel, 'missing <!DOCTYPE html>');
  if (!/<html[^>]*\slang="[a-z-]+"/i.test(html)) err(rel, 'missing <html lang>');
  if (!/<meta name="viewport"/i.test(html)) err(rel, 'missing viewport meta');

  // headings
  const headings = [...body.matchAll(/<h([1-6])\b/gi)].map((m) => Number(m[1]));
  const h1s = headings.filter((h) => h === 1).length;
  if (h1s !== 1) err(rel, `expected exactly one <h1>, found ${h1s}`);
  if (headings[0] !== 1) err(rel, 'first heading is not <h1>');
  for (let i = 1; i < headings.length; i++) {
    if (headings[i] > headings[i - 1] + 1) err(rel, `heading level skips from h${headings[i - 1]} to h${headings[i]}`);
  }

  // meta
  const title = (html.match(/<title>([\s\S]*?)<\/title>/i) || [])[1];
  if (!title) err(rel, 'missing <title>');
  else {
    if (titles.has(title)) err(rel, `duplicate <title> (also ${titles.get(title)})`);
    titles.set(title, rel);
    if (text(title).length > 70) err(rel, `title longer than 70 chars (${text(title).length})`);
  }
  const desc = (html.match(/<meta name="description" content="([^"]*)"/i) || [])[1];
  if (!desc) err(rel, 'missing meta description');
  else {
    if (descriptions.has(desc)) err(rel, `duplicate meta description (also ${descriptions.get(desc)})`);
    descriptions.set(desc, rel);
    if (!noindex && (desc.length < 70 || desc.length > 230)) err(rel, `meta description length ${desc.length} outside 70-230`);
  }
  if (!noindex) {
    const canonical = (html.match(/<link rel="canonical" href="([^"]+)"/i) || [])[1];
    const expected = SITE_URL + urlPathOf(f);
    if (canonical !== expected) err(rel, `canonical ${canonical} should be ${expected}`);
    else indexable.add(expected);
    for (const p of ['og:title', 'og:description', 'og:image', 'og:url', 'og:type']) {
      if (!new RegExp(`<meta property="${p}" content="[^"]+"`).test(html)) err(rel, `missing ${p}`);
    }
    const ogUrl = (html.match(/<meta property="og:url" content="([^"]+)"/) || [])[1];
    if (ogUrl && ogUrl !== expected) err(rel, `og:url ${ogUrl} should be ${expected}`);
    if (!/<meta name="twitter:card" content="summary_large_image"/.test(html)) err(rel, 'missing twitter:card');
  }

  // JSON-LD
  for (const m of html.matchAll(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/g)) {
    try {
      const data = JSON.parse(m[1]);
      const nodes = data['@graph'] || [data];
      if (!data['@context']) err(rel, 'JSON-LD missing @context');
      nodes.forEach((n, i) => { if (!n['@type']) err(rel, `JSON-LD node ${i} missing @type`); });
      if (JSON.stringify(data).includes('__')) err(rel, 'JSON-LD contains an unreplaced __PLACEHOLDER__');
    } catch (e) {
      err(rel, `JSON-LD does not parse: ${e.message}`);
    }
  }
  if (/__[A-Z_]+__/.test(html)) err(rel, 'unreplaced __PLACEHOLDER__ in page');

  // third-party runtime dependencies that this site intentionally avoids
  if (/cdn\.tailwindcss\.com|fonts\.googleapis\.com|fonts\.gstatic\.com/.test(html)) err(rel, 'references Tailwind Play CDN or Google Fonts');

  // images
  for (const m of body.matchAll(/<img\b[^>]*>/gi)) {
    if (attr(m[0], 'alt') === null) err(rel, `<img> without alt: ${m[0].slice(0, 80)}`);
    if (!attr(m[0], 'width') || !attr(m[0], 'height')) err(rel, `<img> without width/height: ${m[0].slice(0, 80)}`);
  }

  // links and assets
  const refs = [];
  for (const m of body.matchAll(/<(a|link|img|source|script)\b[^>]*>/gi)) {
    const tag = m[0];
    for (const a of ['href', 'src']) { const v = attr(tag, a); if (v !== null) refs.push(v); }
    for (const a of ['srcset', 'imagesrcset']) {
      const v = attr(tag, a);
      if (v) v.split(',').forEach((part) => refs.push(part.trim().split(/\s+/)[0]));
    }
  }
  for (const ref of refs) {
    const t = resolveTarget(f, ref);
    if (!t) continue;
    if (t.error) err(rel, `${ref}: ${t.error}`);
    if (!fs.existsSync(t.file)) { err(rel, `broken link ${ref}`); continue; }
    if (t.frag) {
      const target = pages.get(t.file);
      if (target && !target.ids.has(decodeURIComponent(t.frag))) err(rel, `missing anchor ${ref}`);
    }
  }

  // install snippets must match the canonical landing page
  for (const m of body.matchAll(/<pre\b[^>]*data-sync="index"[^>]*>([\s\S]*?)<\/pre>/g)) {
    const code = decode(m[1].replace(/<[^>]+>/g, '')).trim();
    if (!indexCodeBlocks.has(code)) err(rel, `data-sync="index" code block differs from index.html: ${code.slice(0, 60)}`);
  }
}

// sitemap
const sitemapPath = path.join(SITE, 'sitemap.xml');
if (!fs.existsSync(sitemapPath)) err('sitemap.xml', 'missing');
else {
  const xml = fs.readFileSync(sitemapPath, 'utf8');
  const locs = [...xml.matchAll(/<loc>([^<]+)<\/loc>/g)].map((m) => m[1]);
  const lastmods = [...xml.matchAll(/<lastmod>([^<]+)<\/lastmod>/g)].map((m) => m[1]);
  if (lastmods.length !== locs.length || lastmods.some((d) => !/^\d{4}-\d{2}-\d{2}$/.test(d))) err('sitemap.xml', 'every <url> needs a YYYY-MM-DD <lastmod>');
  for (const l of locs) if (!indexable.has(l)) err('sitemap.xml', `lists ${l}, which is not an indexable page`);
  for (const u of indexable) if (!locs.includes(u)) err('sitemap.xml', `missing ${u}`);
}

// root files GitHub Pages must keep serving
const robots = fs.existsSync(path.join(SITE, 'robots.txt')) ? fs.readFileSync(path.join(SITE, 'robots.txt'), 'utf8') : '';
if (!robots.includes(`Sitemap: ${SITE_URL}/sitemap.xml`)) err('robots.txt', 'missing or does not reference the sitemap');
if (!fs.existsSync(path.join(SITE, 'CNAME')) || fs.readFileSync(path.join(SITE, 'CNAME'), 'utf8').trim() !== 'fyr.guide') err('CNAME', 'missing or not fyr.guide');
for (const f of ['install.sh', 'install.ps1', '404.html', 'site.webmanifest']) if (!fs.existsSync(path.join(SITE, f))) err(f, 'missing at site root');

if (errors.length) {
  console.error(`Site check failed with ${errors.length} problem(s):\n  ` + errors.join('\n  '));
  process.exit(1);
}
console.log(`Site check passed: ${htmlFiles.length} HTML pages, ${indexable.size} indexable URLs, ${files.length} files.`);
