import { cpSync, createReadStream, existsSync, readFileSync, statSync } from 'node:fs'
import { dirname, join, normalize, sep } from 'node:path'
import { createRequire } from 'node:module'
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// pdf.js data files (CMaps, standard fonts, JPEG 2000/JBIG2 wasm decoders, ICC
// profiles) are fetched by pdf.js at runtime, only when a PDF needs them. Fyr
// runs offline, so they are served from /static/pdfjs/<version>/ instead of a
// CDN. The version in the path keeps cached copies from going stale on upgrade.
const require = createRequire(import.meta.url)
const pdfjsRoot = dirname(require.resolve('pdfjs-dist/package.json'))
const pdfjsVersion = JSON.parse(readFileSync(join(pdfjsRoot, 'package.json'), 'utf8')).version
const pdfjsAssetDir = `pdfjs/${pdfjsVersion}/`
const PDFJS_DATA_DIRS = ['cmaps', 'standard_fonts', 'wasm', 'iccs']
// quickjs is only used for scripted (JavaScript) PDFs, which Fyr does not run.
const skipPdfjsFile = (path) => /quickjs/.test(path)

const pdfjsAssets = () => {
  let outDir = ''
  return {
    name: 'fyr-pdfjs-assets',
    configResolved(config) {
      outDir = config.build.outDir
    },
    writeBundle() {
      for (const dir of PDFJS_DATA_DIRS) {
        cpSync(join(pdfjsRoot, dir), join(outDir, pdfjsAssetDir, dir), {
          recursive: true,
          filter: (src) => !skipPdfjsFile(src)
        })
      }
    },
    configureServer(server) {
      const prefix = `/static/${pdfjsAssetDir}`
      server.middlewares.use((req, res, next) => {
        const url = (req.url || '').split('?')[0]
        if (!url.startsWith(prefix)) return next()
        const rel = normalize(decodeURIComponent(url.slice(prefix.length)))
        const top = rel.split(sep)[0]
        const file = join(pdfjsRoot, rel)
        if (!PDFJS_DATA_DIRS.includes(top) || rel.includes('..') || skipPdfjsFile(rel)) return next()
        if (!existsSync(file) || !statSync(file).isFile()) return next()
        createReadStream(file).pipe(res)
      })
    }
  }
}

const proxyHost = (() => {
  const configuredHost = process.env.FYR_HOST
  if (!configuredHost || configuredHost === '0.0.0.0' || configuredHost === '::') {
    return '127.0.0.1'
  }
  return configuredHost
})()

const proxyPort = process.env.FYR_PORT || '8080'
const proxyTarget = process.env.FYR_DEV_PROXY_TARGET || `http://${proxyHost}:${proxyPort}`

export default defineConfig({
  base: '/static/',
  plugins: [vue(), pdfjsAssets()],
  define: {
    __FYR_PDFJS_ASSET_DIR__: JSON.stringify(pdfjsAssetDir)
  },
  build: {
    outDir: '../../../public/static',
    emptyOutDir: true,
    rollupOptions: {
      output: {
        entryFileNames: 'js/[name]-[hash].js',
        chunkFileNames: 'js/[name]-[hash].js',
        assetFileNames: (assetInfo) => {
          if (assetInfo.name.endsWith('.css')) {
            return 'css/[name]-[hash].css'
          }
          // Keep the extension so the server sends the right MIME type
          // (module workers such as MapLibre's refuse non-JS types).
          return 'assets/[name]-[hash][extname]'
        }
      }
    }
  },
  test: {
    environment: 'happy-dom',
    include: ['src/**/*.test.js']
  },
  server: {
    proxy: {
      '/api': {
        target: proxyTarget,
        changeOrigin: true
      },
      '/data': {
        target: proxyTarget,
        changeOrigin: true
      },
      '/docs': {
        target: proxyTarget,
        changeOrigin: true
      }
    }
  }
})
