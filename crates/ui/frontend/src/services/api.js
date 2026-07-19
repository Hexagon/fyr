import axios from 'axios'

const API_BASE = '/api'
const REQUEST_TIMEOUT_MS = 15000
const MAX_RETRIES = 2
const RETRYABLE_METHODS = new Set(['get', 'head', 'options'])

const api = axios.create({
  baseURL: API_BASE,
  timeout: REQUEST_TIMEOUT_MS,
  headers: {
    'Content-Type': 'application/json'
  }
})

api.interceptors.response.use(
  (response) => response,
  async (error) => {
    const config = error.config || {}
    const method = String(config.method || 'get').toLowerCase()
    const isRetryableMethod = RETRYABLE_METHODS.has(method)
    const isNetworkError = !error.response
    const isRetriableStatus = error.response?.status >= 500

    if (isRetryableMethod && (isNetworkError || isRetriableStatus)) {
      config.__retryCount = config.__retryCount || 0
      if (config.__retryCount < MAX_RETRIES) {
        config.__retryCount += 1
        const backoffMs = 500 * config.__retryCount
        await new Promise((resolve) => setTimeout(resolve, backoffMs))
        return api(config)
      }
    }

    return Promise.reject(error)
  }
)

// Data transformation mapping (API returns different property names)
const mapContentItem = (item) => ({
  ...item,
  filename: item.name || item.filename,
  size: item.file_size || item.size,
  path: item.file_path || item.path,
  modified: item.created_at || item.modified
})

const mapContentArray = (items) => (items && items.length > 0 ? items.map(mapContentItem) : [])

const decodePathSegmentSafe = (segment) => {
  let value = String(segment || '')
  for (let i = 0; i < 3; i += 1) {
    try {
      const decoded = decodeURIComponent(value)
      if (decoded === value) break
      value = decoded
    } catch {
      break
    }
  }
  return value
}

const encodePathPreservingSlashes = (path) => {
  return String(path || '')
    .replace(/^\/+/, '')
    .split('/')
    .map((segment) => encodeURIComponent(decodePathSegmentSafe(segment)))
    .join('/')
}

export const apiService = {
  getReaderCapabilities: async () => {
    const response = await api.get('/reader/capabilities')
    return response.data || {
      module: 'fyr-unified-reader',
      version: '0.1',
      formats: [],
      legacy_bridge_available: false,
      legacy_bridge_url: ''
    }
  },

  getReaderOpenDescriptor: async (filename) => {
    const response = await api.get(`/reader/open/${encodeURIComponent(filename)}`)
    return response.data
  },

  getZimArchiveMeta: async (filename) => {
    const response = await api.get(`/reader/zim/${encodeURIComponent(filename)}/meta`)
    return response.data
  },

  getZimReaderCapabilities: async (filename) => {
    const response = await api.get(`/reader/zim/${encodeURIComponent(filename)}/capabilities`)
    return response.data
  },

  getZimNativeArticle: async (filename, path = null) => {
    const params = new URLSearchParams()
    if (path) {
      params.set('path', path)
    }

    const query = params.toString()
    const suffix = query ? `?${query}` : ''
    const response = await api.get(`/reader/zim/${encodeURIComponent(filename)}/native/article${suffix}`)
    return response.data
  },

  getZimNativeSearch: async (filename, q, limit = 20) => {
    const params = new URLSearchParams()
    params.set('q', String(q || ''))
    params.set('limit', String(limit))
    const suffix = params.toString()
    const response = await api.get(`/reader/zim/${encodeURIComponent(filename)}/native/search?${suffix}`)
    return response.data
  },

  getZimNativeContentUrl: (filename, path) => {
    const normalizedPath = encodePathPreservingSlashes(path)
    return `/api/reader/zim/${encodeURIComponent(filename)}/native/content/${normalizedPath}`
  },

  // Status & Config
  getStatus: () => api.get('/status'),
  getSettings: () => api.get('/settings'),
  updateSettings: (settings) => api.put('/settings', settings),
  getStorage: () => api.get('/storage'),

  // Content Listing - with data transformation
  getMaps: async () => {
    const response = await api.get('/content/maps')
    return { data: mapContentArray(response.data.value || response.data) }
  },
  getBooks: async () => {
    const response = await api.get('/content/books')
    return { data: mapContentArray(response.data.value || response.data) }
  },
  getPOIs: async () => {
    const response = await api.get('/content/poi')
    return { data: mapContentArray(response.data.value || response.data) }
  },
  getModels: async () => {
    const response = await api.get('/content/models')
    return { data: mapContentArray(response.data.value || response.data) }
  },
  getMisc: async () => {
    const response = await api.get('/content/misc')
    return { data: mapContentArray(response.data.value || response.data) }
  },

  // AI model registry
  listAiModels: () => api.get('/models'),
  uploadModel: async (file) => {
    const formData = new FormData()
    formData.append('file', file)

    const response = await fetch('/api/models/upload', {
      method: 'POST',
      body: formData,
      cache: 'no-store'
    })

    let payload = null
    const responseType = response.headers.get('content-type') || ''

    if (responseType.includes('application/json')) {
      payload = await response.json()
    } else {
      const text = await response.text()
      payload = text ? { message: text } : null
    }

    if (!response.ok) {
      throw {
        response: {
          status: response.status,
          data: payload
        }
      }
    }

    return { data: payload }
  },
  uploadFile: async (file) => {
    const formData = new FormData()
    formData.append('file', file)

    const response = await fetch('/api/import/upload', {
      method: 'POST',
      body: formData,
      cache: 'no-store'
    })

    let payload = null
    const responseType = response.headers.get('content-type') || ''

    if (responseType.includes('application/json')) {
      payload = await response.json()
    } else {
      const text = await response.text()
      payload = text ? { message: text } : null
    }

    if (!response.ok) {
      throw {
        response: {
          status: response.status,
          data: payload
        }
      }
    }

    return { data: payload }
  },
  importModel: (filename, source = 'inbox') => api.post('/models/import', { filename, source }),
  createImportDownload: (filename) => api.post(`/import/download/${encodeURIComponent(filename)}`),
  loadModel: (filename) => api.post(`/models/${encodeURIComponent(filename)}/load`),
  getModelHealth: (filename) => api.get(`/models/${encodeURIComponent(filename)}/health`),

  // SSE token streaming helper
  streamInference: (filename, { prompt, temperature = 0.7, maxTokens = 256 }, handlers = {}) => {
    const params = new URLSearchParams({
      prompt,
      temperature: String(temperature),
      max_tokens: String(maxTokens)
    })
    const url = `/api/models/${encodeURIComponent(filename)}/infer/stream?${params.toString()}`
    const source = new EventSource(url)

    source.addEventListener('token', (event) => {
      handlers.onToken?.(event.data)
    })
    source.addEventListener('done', () => {
      handlers.onDone?.()
      source.close()
    })
    source.onerror = (error) => {
      handlers.onError?.(error)
      source.close()
    }
    return source
  },

  // Download Management
  createDownload: (url) => api.post('/download', { url }),
  cancelDownload: (taskId) => api.delete(`/download/${taskId}`),
  getDownloadStatus: (taskId) => api.get(`/download/${taskId}/status`),
  listDownloads: () => api.get('/downloads'),

  // Error handler
  handleError: (error) => {
    console.error('API Error:', error)
    if (error?.message && !error?.response && !error?.code) {
      return error.message
    }
    if (error.code === 'ECONNABORTED') {
      return `Request timed out after ${REQUEST_TIMEOUT_MS / 1000}s. Check server responsiveness.`
    }
    if (!error.response) {
      return 'Network error. Server may be offline or unreachable.'
    }
    if (error.response) {
      if (error.response.status >= 500) {
        return `Server error (${error.response.status}). Please try again.`
      }
      if (error.response.status === 404) {
        return 'Requested resource was not found.'
      }
      if (error.response.status === 400) {
        return error.response.data?.message || 'Invalid request.'
      }
      return error.response.data?.message || `Request failed (${error.response.status}).`
    }
    return error.message || 'Unknown error'
  }
}
