import axios from 'axios'

const API_BASE = '/api'
const REQUEST_TIMEOUT_MS = 120000
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

const parseSseFrame = (frame) => {
  const lines = String(frame || '').split(/\r?\n/)
  let event = 'message'
  const data = []

  for (const rawLine of lines) {
    const line = rawLine.trimEnd()
    if (!line || line.startsWith(':')) {
      continue
    }

    if (line.startsWith('event:')) {
      event = line.slice(6).trim() || 'message'
      continue
    }

    if (line.startsWith('data:')) {
      const value = line.slice(5)
      data.push(value.startsWith(' ') ? value.slice(1) : value)
    }
  }

  return {
    event,
    data: data.join('\n')
  }
}

const extractNextSseFrame = (buffer) => {
  const match = /\r?\n\r?\n/.exec(buffer)
  if (!match || match.index == null) {
    return null
  }

  const boundaryIndex = match.index
  return {
    frame: buffer.slice(0, boundaryIndex),
    remainder: buffer.slice(boundaryIndex + match[0].length)
  }
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
  getCuratedContent: async () => {
    const response = await axios.get('/data/curated-content.json', {
      timeout: REQUEST_TIMEOUT_MS,
      headers: {
        Accept: 'application/json'
      }
    })
    return { data: response.data || {} }
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
  streamInference: (filename, { prompt, temperature = 0.2, maxTokens = 512, numCtx, history = [] }, handlers = {}) => {
    const params = new URLSearchParams({
      prompt,
      temperature: String(temperature),
      max_tokens: String(maxTokens)
    })
    if (numCtx != null) {
      params.set('num_ctx', String(numCtx))
    }
    if (history.length > 0) {
      params.set('history', JSON.stringify(history))
    }
    const url = `/api/models/${encodeURIComponent(filename)}/infer/stream?${params.toString()}`
    const controller = new AbortController()
    let closed = false

    const close = () => {
      if (closed) return
      closed = true
      controller.abort()
    }

    ;(async () => {
      let completed = false

      try {
        const response = await fetch(url, {
          method: 'GET',
          headers: {
            Accept: 'text/event-stream'
          },
          cache: 'no-store',
          signal: controller.signal
        })

        if (!response.ok) {
          throw {
            response: {
              status: response.status,
              data: { message: await response.text() }
            }
          }
        }

        if (!response.body) {
          throw new Error('Streaming response body is unavailable.')
        }

        const reader = response.body.getReader()
        const decoder = new TextDecoder()
        let buffer = ''

        while (!closed) {
          const { value, done } = await reader.read()
          if (done) {
            break
          }

          buffer += decoder.decode(value, { stream: true })

          let extracted = extractNextSseFrame(buffer)
          while (extracted) {
            const { frame, remainder } = extracted
            buffer = remainder

            const parsed = parseSseFrame(frame)
            if (parsed.event === 'token') {
              handlers.onToken?.(parsed.data)
            } else if (parsed.event === 'done') {
              completed = true
              handlers.onDone?.()
              close()
              return
            }

            extracted = extractNextSseFrame(buffer)
          }
        }

        const trailing = buffer + decoder.decode()
        if (!closed && trailing.trim()) {
          const parsed = parseSseFrame(trailing)
          if (parsed.event === 'token') {
            handlers.onToken?.(parsed.data)
          } else if (parsed.event === 'done') {
            completed = true
            handlers.onDone?.()
          }
        }

        if (!closed && !completed) {
          handlers.onDone?.()
        }
      } catch (error) {
        if (!controller.signal.aborted) {
          handlers.onError?.(error)
        }
      }
    })()

    return { close }
  },

  // Download Management
  createDownload: (url) => api.post('/download', { url }),
  cancelDownload: (taskId) => api.delete(`/download/${taskId}`),
  dismissDownload: (taskId) => api.delete(`/download/${taskId}/dismiss`),
  getDownloadStatus: (taskId) => api.get(`/download/${taskId}/status`),
  listDownloads: () => api.get('/downloads'),
  deleteContentFile: (contentType, filename) => api.delete(`/content/${encodeURIComponent(contentType)}/${encodeURIComponent(filename)}`),

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
