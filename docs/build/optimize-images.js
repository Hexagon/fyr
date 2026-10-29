#!/usr/bin/env node
// Generates responsive AVIF/WebP variants (and the social preview JPEG) for the
// fyr.guide landing page from the source PNGs in docs/site/img/.
//
// Not part of `npm run build` (the outputs are committed); run it only when a
// source image is added or replaced:
//
//   cd docs/build
//   npm install --no-save sharp@0.34.5
//   node optimize-images.js
//
// Then update width/height/srcset in docs/site/index.html if dimensions changed.

const path = require('path');
const fs = require('fs');

let sharp;
try {
  sharp = require('sharp');
} catch {
  console.error('sharp is not installed. Run: npm install --no-save sharp@0.34.5');
  process.exit(1);
}

const IMG = path.resolve(__dirname, '..', 'site', 'img');

const JOBS = [
  // The hero is object-cover scaled to >=1376 CSS px wide at every viewport,
  // so only the full-size variant is useful.
  { src: 'hero.png', widths: [1376] },
  { src: 'screenshot-1.png', widths: [800, 1629] },
  { src: 'screenshot-2.png', widths: [800, 1628] },
  { src: 'screenshot-3.png', widths: [800, 1628] },
  { src: 'screenshot-4.png', widths: [800, 1627] },
];

async function main() {
  for (const job of JOBS) {
    const base = job.src.replace(/\.png$/, '');
    for (const w of job.widths) {
      const pipeline = () => sharp(path.join(IMG, job.src)).resize({ width: w, withoutEnlargement: true });
      const avif = path.join(IMG, `${base}-${w}.avif`);
      const webp = path.join(IMG, `${base}-${w}.webp`);
      await pipeline().avif({ quality: 55, effort: 6 }).toFile(avif);
      await pipeline().webp({ quality: 78, effort: 6 }).toFile(webp);
      console.log(`${path.basename(avif)} ${fs.statSync(avif).size}  ${path.basename(webp)} ${fs.statSync(webp).size}`);
    }
  }
  // Social preview (Open Graph / Twitter): JPEG is the most widely supported.
  const og = path.join(IMG, 'og-image.jpg');
  await sharp(path.join(IMG, 'screenshot-1.png'))
    .resize({ width: 1200, height: 630, fit: 'cover', position: 'centre' })
    .flatten({ background: '#111827' })
    .jpeg({ quality: 82, mozjpeg: true })
    .toFile(og);
  console.log(`og-image.jpg ${fs.statSync(og).size}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
