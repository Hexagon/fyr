#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

const ROOT_DIR = path.resolve(__dirname, '..', '..');
const BOOKS_DIR = path.join(ROOT_DIR, 'public', 'data', 'books');

const MANUALS = [
  {
    id: 'user',
    source: path.join(ROOT_DIR, 'docs', 'user', 'USER_MANUAL.md'),
    output: path.join(BOOKS_DIR, 'user-manual.md'),
  },
  {
    id: 'developer',
    source: path.join(ROOT_DIR, 'docs', 'developer', 'DEVELOPER_MANUAL.md'),
    output: path.join(BOOKS_DIR, 'developer-manual.md'),
  },
];
function copyManual(manual) {
  if (!fs.existsSync(manual.source)) {
    throw new Error(`Manual source not found: ${manual.source}`);
  }

  if (!fs.existsSync(BOOKS_DIR)) {
    fs.mkdirSync(BOOKS_DIR, { recursive: true });
  }

  fs.copyFileSync(manual.source, manual.output);
}

async function main() {
  console.log('Copying documentation manuals into public/data/books/...');

  for (const manual of MANUALS) {
    copyManual(manual);
    const sizeBytes = fs.statSync(manual.output).size;
    console.log(`Copied ${path.basename(manual.output)} (${(sizeBytes / 1024).toFixed(2)} KB)`);
  }

  console.log('Done. Markdown manuals are available in public/data/books/.');
}

main().catch((error) => {
  console.error('Failed to build manuals:', error.message || error);
  process.exit(1);
});
