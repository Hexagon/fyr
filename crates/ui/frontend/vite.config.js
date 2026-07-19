import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

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
  plugins: [vue()],
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
          return 'assets/[name]-[hash]'
        }
      }
    }
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
