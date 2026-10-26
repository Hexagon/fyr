import { ref } from 'vue'
import { useEpubReader } from './useEpubReader'
import { createReaderFactory } from './useReaderFactory'
import { useMarkdownReader } from './useMarkdownReader'
import { usePdfReader } from './usePdfReader'
import { useZimReader } from './useZimReader'

export const useUnifiedReader = () => {
  const activeFormat = ref('none')
  const status = ref('idle')
  const error = ref(null)
  const zimStartPathFailed = ref(false)

  const epubReader = useEpubReader()
  const markdownReader = useMarkdownReader()
  const pdfReader = usePdfReader()
  const zimReader = useZimReader()

  const factory = createReaderFactory({
    epub: epubReader,
    md: markdownReader,
    pdf: pdfReader,
    zim: zimReader
  })

  // Each selectBook() gets a fresh token; selecting another book (or
  // disposing) cancels the previous one so its late async steps are ignored.
  let currentToken = null

  const cancelCurrent = () => {
    if (currentToken) currentToken.cancelled = true
    currentToken = null
  }

  const resetContentState = () => {
    epubReader.dispose()
    markdownReader.dispose()
    pdfReader.dispose()
    zimReader.dispose()
  }

  const resetForSelection = () => {
    error.value = null
    status.value = 'idle'
    zimStartPathFailed.value = false
    resetContentState()
  }

  const openBook = async (book, apiService, token, options) => {
    const descriptor = await apiService
      .getReaderOpenDescriptor(book?.filename)
      .catch(() => factory.inferDescriptorFromSelection(book))
    if (token.cancelled) return

    const format = descriptor?.format || factory.detectFormat(book?.filename)
    activeFormat.value = format
    error.value = null

    const reader = factory.getReader(format)
    if (!reader) {
      status.value = 'unsupported format'
      return
    }

    status.value = `loading ${descriptor.filename}`

    if (format === 'zim') {
      const result = await reader.open(descriptor, apiService, token, { startPath: options.zimPath })
      if (token.cancelled) return
      zimStartPathFailed.value = Boolean(result?.startPathFailed)
    } else if (format === 'epub') {
      await reader.open(descriptor, token, {
        startCfi: options.epubCfi,
        prefs: options.prefs,
        onRelocated: options.onEpubRelocated
      })
    } else {
      await reader.open(descriptor, token)
    }
    if (token.cancelled) return

    status.value = `opened ${descriptor.filename}`
  }

  /**
   * Open `book`. Options: { zimPath, epubCfi, prefs, onEpubRelocated }.
   * Resolves to true when this selection is still current after loading.
   */
  const selectBook = async (book, apiService, options = {}) => {
    cancelCurrent()
    const token = { cancelled: false }
    currentToken = token
    resetForSelection()

    try {
      await openBook(book, apiService, token, options)
    } catch (err) {
      if (token.cancelled) return false
      error.value = apiService.handleError(err)
      status.value = 'failed'
    }
    return !token.cancelled
  }

  const loadNativeZimArticle = async (filename, path, apiService) => {
    return zimReader.loadNativeArticle(filename, path, apiService)
  }

  const dispose = () => {
    cancelCurrent()
    resetContentState()
    activeFormat.value = 'none'
    status.value = 'idle'
    error.value = null
    zimStartPathFailed.value = false
  }

  return {
    activeFormat,
    status,
    error,
    epubBook: epubReader.book,
    epubPosition: epubReader.position,
    epubGoTo: epubReader.goTo,
    epubApplyPrefs: epubReader.applyPrefs,
    markdownHtml: markdownReader.html,
    zimMeta: zimReader.meta,
    zimAdapter: zimReader.adapter,
    zimNativeArticle: zimReader.nativeArticle,
    zimPendingHash: zimReader.pendingHash,
    zimStartPathFailed,
    pdfUrl: pdfReader.url,
    hasExtension: factory.hasExtension,
    decodePathDeep: zimReader.decodePathDeep,
    selectBook,
    loadNativeZimArticle,
    dispose
  }
}
