#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

const repoRoot = path.resolve(__dirname, '..', '..');
const kiwixDir = path.join(repoRoot, 'public', 'kiwix-static');

const requiredFiles = [
  'LICENSE-GPLv3.txt',
  'LICENSE-AGPLv3.txt',
  'THIRD_PARTY_NOTICES.txt',
  'KIWIX_SOURCE_MANIFEST.json',
  path.join('www', 'index.html'),
  'service-worker.js',
  'replayWorker.js'
];

const errors = [];

for (const relativePath of requiredFiles) {
  const absolutePath = path.join(kiwixDir, relativePath);
  if (!fs.existsSync(absolutePath)) {
    errors.push('Missing required file: public/kiwix-static/' + relativePath.replace(/\\/g, '/'));
  }
}

const manifestPath = path.join(kiwixDir, 'KIWIX_SOURCE_MANIFEST.json');
if (fs.existsSync(manifestPath)) {
  let manifest;
  try {
    manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  } catch (error) {
    errors.push('Invalid JSON in public/kiwix-static/KIWIX_SOURCE_MANIFEST.json: ' + error.message);
  }

  if (manifest) {
    const requiredKeys = [
      'manifest_version',
      'component',
      'upstream_repository',
      'upstream_release_tag',
      'upstream_archive_url',
      'source_code_url',
      'bundle_runtime_markers',
      'subset_policy',
      'last_reviewed_utc'
    ];

    for (const key of requiredKeys) {
      if (!(key in manifest)) {
        errors.push('Missing key in KIWIX_SOURCE_MANIFEST.json: ' + key);
      }
    }

    if (manifest.upstream_repository && !String(manifest.upstream_repository).includes('kiwix-js')) {
      errors.push('upstream_repository should reference kiwix-js.');
    }

    if (manifest.bundle_runtime_markers && !manifest.bundle_runtime_markers.appVersion) {
      errors.push('bundle_runtime_markers.appVersion is required.');
    }

    if (manifest.subset_policy && !manifest.subset_policy.distribution_path) {
      errors.push('subset_policy.distribution_path is required.');
    }
  }
}

const noticesPath = path.join(kiwixDir, 'THIRD_PARTY_NOTICES.txt');
if (fs.existsSync(noticesPath)) {
  const notices = fs.readFileSync(noticesPath, 'utf8');
  if (!/GPL/i.test(notices)) {
    errors.push('THIRD_PARTY_NOTICES.txt should mention GPL.');
  }
  if (!/AGPL/i.test(notices)) {
    errors.push('THIRD_PARTY_NOTICES.txt should mention AGPL.');
  }
}

if (errors.length > 0) {
  console.error('Kiwix compliance verification failed:');
  for (const error of errors) {
    console.error('- ' + error);
  }
  process.exit(1);
}

console.log('Kiwix compliance verification passed.');
console.log('Verified files in public/kiwix-static and source manifest metadata.');
