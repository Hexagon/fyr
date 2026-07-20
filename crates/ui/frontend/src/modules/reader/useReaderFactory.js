const hasExtension = (filename, extension) => String(filename || '').toLowerCase().endsWith(extension)

const detectFormat = (filename) => {
  if (hasExtension(filename, '.zim')) return 'zim'
  if (hasExtension(filename, '.epub')) return 'epub'
  if (hasExtension(filename, '.md')) return 'md'
  if (hasExtension(filename, '.pdf')) return 'pdf'
  return 'unknown'
}

const inferDescriptorFromSelection = (book) => {
  const format = detectFormat(book?.filename)
  const encoded = encodeURIComponent(book?.filename || '')

  return {
    filename: book?.filename || '',
    format,
    content_url: `/docs/books/${encoded}`,
    meta_url: format === 'zim' ? `/api/reader/zim/${encoded}/meta` : null,
    supports_search: format === 'zim',
    supports_navigation: ['zim', 'epub', 'md', 'pdf'].includes(format),
    supports_inline_render: ['epub', 'md', 'pdf'].includes(format)
  }
}

export const createReaderFactory = (readers) => ({
  detectFormat,
  hasExtension,
  inferDescriptorFromSelection,
  getReader(format) {
    return readers[format] || null
  }
})
