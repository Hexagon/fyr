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
    content_url: `/docs/books/${encoded}`
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
