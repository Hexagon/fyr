import axios from 'axios'

const API_BASE = '/api'

const api = axios.create({
  baseURL: API_BASE,
  headers: {
    'Content-Type': 'application/json'
  }
})

// Data transformation mapping (API returns different property names)
const mapContentItem = (item) => ({
  ...item,
  filename: item.name || item.filename,
  size: item.file_size || item.size,
  path: item.file_path || item.path,
  modified: item.created_at || item.modified
})

const mapContentArray = (items) => (items && items.length > 0 ? items.map(mapContentItem) : [])

export const apiService = {
  getZimReaderConfig: () => ({
    localUrl: '/kiwix/www/index.html'
  }),

  getKiwixStatus: async () => {
    const response = await api.get('/kiwix/status')
    return response.data || { available: false, local_url: '/kiwix/www/index.html' }
  },

  getKiwixReaderCapabilities: async () => {
    const response = await api.get('/reader/kiwix/capabilities')
    return response.data || {
      available: false,
      local_url: '/kiwix/www/index.html',
      zim_base_url: '/data/books',
      supports_direct_http_zim: false,
      supports_http_range: false
    }
  },

  checkLocalZimReader: async () => {
    try {
      const status = await apiService.getKiwixStatus()
      if (status.available) return true
    } catch (error) {
      console.warn('Kiwix status endpoint unavailable, falling back to direct check:', error)
    }

    try {
      const response = await fetch('/kiwix/www/index.html', {
        method: 'GET',
        cache: 'no-store'
      })
      return response.ok
    } catch {
      return false
    }
  },

  // Status & Config
  getStatus: () => api.get('/status'),
  getConfig: () => api.get('/config'),
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
  importModel: (filename, source = 'inbox') => api.post('/models/import', { filename, source }),
  loadModel: (filename) => api.post(`/models/${encodeURIComponent(filename)}/load`),
  unloadModel: (filename) => api.delete(`/models/${encodeURIComponent(filename)}/load`),
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
    source.onerror = (error) => {
      handlers.onError?.(error)
      source.close()
    }
    return source
  },

  // Download Management
  createDownload: (url) => api.post('/download', { url }),
  getDownloadStatus: (taskId) => api.get(`/download/${taskId}/status`),
  listDownloads: () => api.get('/downloads'),

  // Error handler
  handleError: (error) => {
    console.error('API Error:', error)
    if (error.response) {
      return error.response.data?.message || `Error ${error.response.status}`
    }
    return error.message || 'Unknown error'
  }
}
