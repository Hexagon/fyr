import { describe, expect, it, vi } from 'vitest'

vi.mock('epubjs', () => ({ default: vi.fn() }))

const { useZimReader } = await import('./useZimReader')
const { useUnifiedReader } = await import('./useUnifiedReader')

const deferred = () => {
  let resolve
  let reject
  const promise = new Promise((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

const article = (path) => ({ path, title: path, content: `<p>${path}</p>` })

const zimApi = (overrides = {}) => ({
  getZimArchiveMeta: async () => ({ size_bytes: 1 }),
  getZimReaderCapabilities: async () => ({ mode: 'native', supports_native_render: true }),
  getZimNativeArticle: async (_f, path) => article(path || 'A/Main'),
  getZimNativeContentUrl: (f, p) => `/api/reader/zim/${f}/native/content/${p}`,
  getReaderOpenDescriptor: async (filename) => ({ filename, format: filename.split('.').pop(), content_url: `/docs/books/${filename}` }),
  handleError: (e) => String(e?.message || e),
  ...overrides
})

describe('ZIM article loads', () => {
  it('a slow earlier article never replaces a newer one', async () => {
    const slow = deferred()
    const api = zimApi({
      getZimNativeArticle: (_f, path) => (path === 'A/Slow' ? slow.promise : Promise.resolve(article(path)))
    })
    const reader = useZimReader()
    const first = reader.loadNativeArticle('w.zim', 'A/Slow', api)
    const second = await reader.loadNativeArticle('w.zim', 'A/Fast', api)
    expect(second.path).toBe('A/Fast')
    slow.resolve(article('A/Slow'))
    expect(await first).toBeNull()
    expect(reader.nativeArticle.value.path).toBe('A/Fast')
  })

  it('keeps the hash for scrolling and reports it on the article', async () => {
    const reader = useZimReader()
    const loaded = await reader.loadNativeArticle('w.zim', 'A/Earth#History', zimApi())
    expect(loaded.path).toBe('A/Earth')
    expect(loaded.requested_hash).toBe('#History')
    expect(reader.pendingHash.value).toBe('#History')
  })

  it('falls back to the main page when the start path is missing', async () => {
    const api = zimApi({
      getZimNativeArticle: async (_f, path) => {
        if (path === 'A/Gone') throw new Error('404')
        return article(path || 'A/Main')
      }
    })
    const reader = useZimReader()
    const result = await reader.open({ filename: 'w.zim' }, api, { cancelled: false }, { startPath: 'A/Gone' })
    expect(result.startPathFailed).toBe(true)
    expect(reader.nativeArticle.value.path).toBe('A/Main')
  })

  it('dispose discards in-flight loads', async () => {
    const slow = deferred()
    const reader = useZimReader()
    const pending = reader.loadNativeArticle('w.zim', 'A/X', zimApi({ getZimNativeArticle: () => slow.promise }))
    reader.dispose()
    slow.resolve(article('A/X'))
    expect(await pending).toBeNull()
    expect(reader.nativeArticle.value).toBeNull()
  })
})

describe('unified reader book switching', () => {
  it('a slow first book cannot overwrite the book selected after it', async () => {
    const slowDescriptor = deferred()
    const api = zimApi({
      getReaderOpenDescriptor: (filename) =>
        filename === 'slow.zim'
          ? slowDescriptor.promise
          : Promise.resolve({ filename, format: 'zim', content_url: `/docs/books/${filename}` })
    })
    const reader = useUnifiedReader()
    const first = reader.selectBook({ filename: 'slow.zim' }, api)
    const second = await reader.selectBook({ filename: 'fast.zim' }, api, { zimPath: 'A/Fast' })
    expect(second).toBe(true)
    expect(reader.zimNativeArticle.value.path).toBe('A/Fast')

    slowDescriptor.resolve({ filename: 'slow.zim', format: 'zim', content_url: '/docs/books/slow.zim' })
    expect(await first).toBe(false)
    expect(reader.status.value).toBe('opened fast.zim')
    expect(reader.zimNativeArticle.value.path).toBe('A/Fast')
  })

  it('errors from a superseded selection are not shown', async () => {
    const failing = deferred()
    const api = zimApi({
      getZimArchiveMeta: (filename) => (filename === 'bad.zim' ? failing.promise : Promise.resolve({ size_bytes: 1 }))
    })
    const reader = useUnifiedReader()
    const first = reader.selectBook({ filename: 'bad.zim' }, api)
    // Let the first selection get as far as the (pending) archive meta request.
    await new Promise((resolve) => setTimeout(resolve, 0))
    await reader.selectBook({ filename: 'good.zim' }, api)
    failing.reject(new Error('boom'))
    expect(await first).toBe(false)
    expect(reader.error.value).toBeNull()
    expect(reader.status.value).toBe('opened good.zim')
  })
})
