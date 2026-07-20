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

  const resetContentState = () => {
    epubReader.dispose()
    markdownReader.dispose()
    pdfReader.dispose()
    zimReader.dispose()
  }

  const resetForSelection = () => {
    error.value = null
    status.value = 'idle'
    resetContentState()
  }

  const openBook = async (book, apiService) => {
    const descriptor = await apiService
      .getReaderOpenDescriptor(book?.filename)
      .catch(() => factory.inferDescriptorFromSelection(book))

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
      await reader.open(descriptor, apiService)
    } else {
      await reader.open(descriptor)
    }

    status.value = `opened ${descriptor.filename}`
  }

  const selectBook = async (book, apiService) => {
    resetForSelection()

    try {
      await openBook(book, apiService)
    } catch (err) {
      error.value = apiService.handleError(err)
      status.value = 'failed'
    }
  }

  const loadNativeZimArticle = async (filename, path, apiService) => {
    return zimReader.loadNativeArticle(filename, path, apiService)
  }

  const resize = () => {
    epubReader.resize()
  }

  const dispose = () => {
    resetContentState()
    activeFormat.value = 'none'
    status.value = 'idle'
    error.value = null
  }

  return {
    activeFormat,
    status,
    error,
    epubBook: epubReader.book,
    markdownHtml: markdownReader.html,
    zimMeta: zimReader.meta,
    zimAdapter: zimReader.adapter,
    zimNativeArticle: zimReader.nativeArticle,
    pdfUrl: pdfReader.url,
    hasExtension: factory.hasExtension,
    decodePathDeep: zimReader.decodePathDeep,
    selectBook,
    loadNativeZimArticle,
    resize,
    dispose
  }
}
