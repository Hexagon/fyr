#!/usr/bin/env node

const fs = require('fs');
const path = require('path');
const archiverLib = require('archiver');

const ROOT_DIR = path.resolve(__dirname, '..', '..');
const BOOKS_DIR = path.join(ROOT_DIR, 'public', 'data', 'books');

const MANUALS = [
  {
    id: 'user',
    title: 'Fyr User Manual',
    source: path.join(ROOT_DIR, 'docs', 'user', 'USER_MANUAL.md'),
    output: path.join(BOOKS_DIR, 'user-manual.epub'),
    chapterFile: 'user-manual.xhtml',
  },
  {
    id: 'developer',
    title: 'Fyr Developer Manual',
    source: path.join(ROOT_DIR, 'docs', 'developer', 'DEVELOPER_MANUAL.md'),
    output: path.join(BOOKS_DIR, 'developer-manual.epub'),
    chapterFile: 'developer-manual.xhtml',
  },
];

function escapeHtml(text) {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function markdownToXhtml(markdown) {
  const lines = markdown.split(/\r?\n/);
  const parts = [];
  let inList = false;
  let inCode = false;
  let codeBuffer = [];

  const closeListIfNeeded = () => {
    if (inList) {
      parts.push('</ul>');
      inList = false;
    }
  };

  const flushCode = () => {
    if (inCode) {
      const code = escapeHtml(codeBuffer.join('\n'));
      parts.push(`<pre><code>${code}</code></pre>`);
      inCode = false;
      codeBuffer = [];
    }
  };

  for (const rawLine of lines) {
    const line = rawLine.trimEnd();

    if (line.startsWith('```')) {
      closeListIfNeeded();
      if (inCode) {
        flushCode();
      } else {
        inCode = true;
      }
      continue;
    }

    if (inCode) {
      codeBuffer.push(rawLine);
      continue;
    }

    const headingMatch = line.match(/^(#{1,6})\s+(.*)$/);
    if (headingMatch) {
      closeListIfNeeded();
      const level = Math.min(headingMatch[1].length, 6);
      parts.push(`<h${level}>${escapeHtml(headingMatch[2])}</h${level}>`);
      continue;
    }

    const listMatch = line.match(/^[-*]\s+(.*)$/);
    if (listMatch) {
      if (!inList) {
        parts.push('<ul>');
        inList = true;
      }
      parts.push(`<li>${escapeHtml(listMatch[1])}</li>`);
      continue;
    }

    if (line.length === 0) {
      closeListIfNeeded();
      continue;
    }

    closeListIfNeeded();
    parts.push(`<p>${escapeHtml(line)}</p>`);
  }

  flushCode();
  closeListIfNeeded();

  return parts.join('\n');
}

function chapterTemplate(title, body) {
  return `<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
  <meta charset="utf-8" />
  <title>${escapeHtml(title)}</title>
  <link rel="stylesheet" href="style.css" type="text/css" />
</head>
<body>
${body}
</body>
</html>`;
}

function packageOpf(title, chapterFile) {
  return `<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="bookid">${escapeHtml(title.toLowerCase().replace(/\s+/g, '-'))}</dc:identifier>
    <dc:title>${escapeHtml(title)}</dc:title>
    <dc:language>en</dc:language>
    <meta property="dcterms:modified">2026-07-17T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="chapter" href="${chapterFile}" media-type="application/xhtml+xml"/>
    <item id="css" href="style.css" media-type="text/css"/>
  </manifest>
  <spine>
    <itemref idref="chapter"/>
  </spine>
</package>`;
}

function navDoc(title, chapterFile) {
  return `<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
  <meta charset="utf-8" />
  <title>${escapeHtml(title)} Navigation</title>
</head>
<body>
  <nav epub:type="toc">
    <h1>${escapeHtml(title)}</h1>
    <ol>
      <li><a href="${chapterFile}">${escapeHtml(title)}</a></li>
    </ol>
  </nav>
</body>
</html>`;
}

function createZipArchive() {
  if (typeof archiverLib === 'function') {
    return archiverLib('zip', { zlib: { level: 9 } });
  }

  if (archiverLib && typeof archiverLib.create === 'function') {
    return archiverLib.create('zip', { zlib: { level: 9 } });
  }

  if (archiverLib && typeof archiverLib.ZipArchive === 'function') {
    return new archiverLib.ZipArchive();
  }

  throw new Error('Unsupported archiver module API');
}

function baseStyle() {
  return `body { font-family: Georgia, serif; line-height: 1.5; padding: 1rem; color: #222; }
h1, h2, h3 { margin-top: 1rem; margin-bottom: 0.5rem; }
p, li { margin-bottom: 0.4rem; }
code { font-family: Consolas, monospace; background: #f3f3f3; padding: 0.1rem 0.25rem; }
pre { background: #f3f3f3; padding: 0.75rem; overflow-x: auto; }`;
}

async function buildManual(manual) {
  if (!fs.existsSync(manual.source)) {
    throw new Error(`Manual source not found: ${manual.source}`);
  }

  const markdown = fs.readFileSync(manual.source, 'utf8');
  const body = markdownToXhtml(markdown);
  const chapter = chapterTemplate(manual.title, body);

  if (!fs.existsSync(BOOKS_DIR)) {
    fs.mkdirSync(BOOKS_DIR, { recursive: true });
  }

  return new Promise((resolve, reject) => {
    const output = fs.createWriteStream(manual.output);
    const archive = createZipArchive();

    output.on('close', () => resolve());
    output.on('error', reject);
    archive.on('error', reject);

    archive.pipe(output);

    archive.append('application/epub+zip', { name: 'mimetype', store: true });
    archive.append(
      `<?xml version="1.0" encoding="UTF-8"?>\n<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">\n  <rootfiles>\n    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>\n  </rootfiles>\n</container>`,
      { name: 'META-INF/container.xml' }
    );

    archive.append(packageOpf(manual.title, manual.chapterFile), { name: 'OEBPS/content.opf' });
    archive.append(navDoc(manual.title, manual.chapterFile), { name: 'OEBPS/nav.xhtml' });
    archive.append(baseStyle(), { name: 'OEBPS/style.css' });
    archive.append(chapter, { name: `OEBPS/${manual.chapterFile}` });

    archive.finalize();
  });
}

async function main() {
  console.log('Building documentation EPUB manuals...');

  for (const manual of MANUALS) {
    await buildManual(manual);
    const sizeBytes = fs.statSync(manual.output).size;
    console.log(`Created ${path.basename(manual.output)} (${(sizeBytes / 1024).toFixed(2)} KB)`);
  }

  console.log('Done. Manuals are available in public/data/books/.');
}

main().catch((error) => {
  console.error('Failed to build manuals:', error.message || error);
  process.exit(1);
});
