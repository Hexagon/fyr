import { describe, expect, it, vi } from 'vitest'

vi.mock('epubjs', () => ({ default: vi.fn() }))

// pdf.js itself is mocked: these tests cover the document lifecycle
// (stale loads, cleanup, errors), not rendering.
const loader = vi.hoisted(() => ({ getDocument: null }))
vi.mock('./pdfjsLoader', () => ({
  loadPdfjs: async () => ({ getDocument: (...args) => loader.getDocument(...args) }),
  documentOptions: (url) => ({ url })
}))

const { usePdfReader, describePdfError } = await import('./usePdfReader')
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

const fakeDoc = (name, numPages = 3) => ({
  name,
  numPages,
  getPage: vi.fn(async (n) => ({
    getTextContent: async () => ({ items: [{ str: `${name} page ${n}`, hasEOL: false }] })
  }))
})

/** getDocument mock: each URL gets its own deferred loading task. */
const tasksByUrl = () => {
  const tasks = new Map()
  loader.getDocument = vi.fn(({ url }) => {
    const d = deferred()
    const task = { promise: d.promise, destroy: vi.fn(async () => {}), resolve: d.resolve, reject: d.reject }
    tasks.set(url, task)
    return task
  })
  return tasks
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

const api = {
  getReaderOpenDescriptor: async (filename) => ({
    filename,
    format: filename.split('.').pop(),
    content_url: `/docs/books/${filename}`
  }),
  handleError: (e) => String(e?.message || e)
}

describe('usePdfReader', () => {
  it('opens a document and exposes page count and page text', async () => {
    const tasks = tasksByUrl()
    const reader = usePdfReader()
    const opening = reader.open({ content_url: '/docs/books/a.pdf' }, { cancelled: false })
    await flush()
    tasks.get('/docs/books/a.pdf').resolve(fakeDoc('a', 4))
    await opening
    expect(reader.pageCount.value).toBe(4)
    expect((await reader.getPageText(2)).text).toBe('a page 2')
    // cached: the page is only fetched once
    await reader.getPageText(2)
    expect(reader.doc.value.getPage).toHaveBeenCalledTimes(1)
  })

  it('a late load after cancellation is destroyed, not shown', async () => {
    const tasks = tasksByUrl()
    const reader = usePdfReader()
    const token = { cancelled: false }
    const opening = reader.open({ content_url: '/docs/books/slow.pdf' }, token)
    await flush()
    token.cancelled = true
    const task = tasks.get('/docs/books/slow.pdf')
    task.resolve(fakeDoc('slow'))
    await opening
    await flush()
    expect(reader.doc.value).toBeNull()
    expect(task.destroy).toHaveBeenCalled()
  })

  it('dispose destroys the in-flight loading task and clears state', async () => {
    const tasks = tasksByUrl()
    const reader = usePdfReader()
    const opening = reader.open({ content_url: '/docs/books/a.pdf' }, { cancelled: false })
    await flush()
    reader.dispose()
    await flush()
    const task = tasks.get('/docs/books/a.pdf')
    expect(task.destroy).toHaveBeenCalled()
    task.reject(Object.assign(new Error('Loading aborted'), { name: 'AbortException' }))
    await expect(opening).resolves.toBeUndefined()
    expect(reader.doc.value).toBeNull()
    expect(reader.pageCount.value).toBe(0)
    expect(await reader.getPageText(1)).toBeNull()
  })

  it('turns pdf.js failures into clear messages', async () => {
    const tasks = tasksByUrl()
    const reader = usePdfReader()
    const opening = reader.open({ content_url: '/docs/books/locked.pdf' }, { cancelled: false })
    await flush()
    tasks.get('/docs/books/locked.pdf').reject(Object.assign(new Error('No password given'), { name: 'PasswordException' }))
    await expect(opening).rejects.toThrow(/password-protected/)
    expect(tasks.get('/docs/books/locked.pdf').destroy).toHaveBeenCalled()
  })
})

describe('describePdfError', () => {
  it('covers encrypted, corrupt, missing and unknown failures', () => {
    expect(describePdfError({ name: 'PasswordException' })).toMatch(/encrypted/)
    expect(describePdfError({ name: 'InvalidPDFException' })).toMatch(/not a valid PDF or it is damaged/)
    expect(describePdfError({ name: 'ResponseException', status: 404, missing: true })).toMatch(/not found/)
    expect(describePdfError({ name: 'ResponseException', status: 500 })).toMatch(/HTTP 500/)
    expect(describePdfError(new Error('boom'))).toBe('Could not open this PDF: boom')
    expect(describePdfError(null)).toBe('Could not open this PDF.')
  })
})

describe('unified reader: quick switching with PDFs', () => {
  it('a slow PDF cannot replace the PDF selected after it', async () => {
    const tasks = tasksByUrl()
    const reader = useUnifiedReader()
    const first = reader.selectBook({ filename: 'slow.pdf' }, api)
    await flush()
    const second = reader.selectBook({ filename: 'fast.pdf' }, api)
    await flush()
    tasks.get('/docs/books/fast.pdf').resolve(fakeDoc('fast'))
    expect(await second).toBe(true)

    tasks.get('/docs/books/slow.pdf').resolve(fakeDoc('slow'))
    expect(await first).toBe(false)
    await flush()
    expect(reader.pdfDoc.value.name).toBe('fast')
    expect(reader.status.value).toBe('opened fast.pdf')
    expect(tasks.get('/docs/books/slow.pdf').destroy).toHaveBeenCalled()
  })

  it('switching from a PDF to Markdown frees the PDF and ignores its late error', async () => {
    const tasks = tasksByUrl()
    const originalFetch = globalThis.fetch
    globalThis.fetch = vi.fn(async () => ({ ok: true, text: async () => '# Notes\n\nhello' }))
    try {
      const reader = useUnifiedReader()
      const first = reader.selectBook({ filename: 'big.pdf' }, api)
      await flush()
      const second = await reader.selectBook({ filename: 'notes.md' }, api)
      expect(second).toBe(true)
      const task = tasks.get('/docs/books/big.pdf')
      expect(task.destroy).toHaveBeenCalled()
      task.reject(Object.assign(new Error('bad'), { name: 'InvalidPDFException' }))
      expect(await first).toBe(false)
      expect(reader.error.value).toBeNull()
      expect(reader.activeFormat.value).toBe('md')
      expect(reader.pdfDoc.value).toBeNull()
      expect(reader.markdownHtml.value).toContain('hello')
    } finally {
      globalThis.fetch = originalFetch
    }
  })

  it('shows the error of the current PDF', async () => {
    const tasks = tasksByUrl()
    const reader = useUnifiedReader()
    const opening = reader.selectBook({ filename: 'broken.pdf' }, api)
    await flush()
    tasks.get('/docs/books/broken.pdf').reject(Object.assign(new Error('Invalid PDF structure'), { name: 'InvalidPDFException' }))
    expect(await opening).toBe(true)
    expect(reader.status.value).toBe('failed')
    expect(reader.error.value).toMatch(/damaged/)
  })
})
