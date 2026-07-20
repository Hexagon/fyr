import { nextTick, ref } from 'vue'
import EPub from 'epubjs'

export const useEpubReader = () => {
  const book = ref(null)
  const rendition = ref(null)

  const dispose = () => {
    if (rendition.value) {
      try {
        rendition.value.destroy()
      } catch {
        // Best-effort cleanup for epubjs internals.
      }
      rendition.value = null
    }

    if (book.value) {
      try {
        book.value.destroy()
      } catch {
        // Best-effort cleanup for epubjs internals.
      }
      book.value = null
    }
  }

  const open = async (descriptor) => {
    const bookObj = new EPub(descriptor.content_url)
    await bookObj.ready

    await nextTick()

    const renditionObj = bookObj.renderTo('book-viewer', {
      width: '100%',
      height: '100%',
      manager: 'continuous',
      flow: 'scrolled-doc',
      spread: 'none',
      allowScriptedContent: false
    })

    renditionObj.flow('scrolled-doc')
    renditionObj.spread('none')

    book.value = bookObj
    rendition.value = renditionObj

    await renditionObj.display()

    renditionObj.themes.default({
      html: {
        'background-color': '#ffffff'
      },
      body: {
        'background-color': '#ffffff',
        color: '#111111'
      }
    })
    renditionObj.themes.select('default')
  }

  const resize = () => {
    rendition.value?.resize()
  }

  return {
    book,
    open,
    resize,
    dispose
  }
}
