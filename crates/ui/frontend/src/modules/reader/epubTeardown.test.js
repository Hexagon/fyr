import { describe, expect, it, vi } from 'vitest'
import { destroyWhenSettled } from './useEpubReader.js'

const deferred = () => {
  let resolve
  let reject
  const promise = new Promise((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

const fakeBook = (extra = {}) => ({ destroy: vi.fn(), loaded: {}, ...extra })

describe('destroyWhenSettled', () => {
  it('destroys immediately when nothing is in flight', async () => {
    const rendition = { destroy: vi.fn() }
    const book = fakeBook()
    await destroyWhenSettled(rendition, book)
    expect(rendition.destroy).toHaveBeenCalledTimes(1)
    expect(book.destroy).toHaveBeenCalledTimes(1)
  })

  it('waits for display, background loaders and book.opened before destroying', async () => {
    const display = deferred()
    const navigation = deferred()
    const opened = deferred()
    const rendition = { destroy: vi.fn() }
    const book = fakeBook({ loaded: { navigation: navigation.promise }, opened: opened.promise })

    const done = destroyWhenSettled(rendition, book, [display.promise])
    display.resolve()
    navigation.resolve()
    await Promise.resolve()
    expect(book.destroy).not.toHaveBeenCalled()

    opened.reject(new Error('aborted')) // rejections count as settled
    await done
    expect(rendition.destroy).toHaveBeenCalledTimes(1)
    expect(book.destroy).toHaveBeenCalledTimes(1)
  })

  it('still destroys after the settle timeout if epub.js never finishes', async () => {
    vi.useFakeTimers()
    try {
      const book = fakeBook({ opened: new Promise(() => {}) })
      const done = destroyWhenSettled(null, book, [], 100)
      await vi.advanceTimersByTimeAsync(99)
      expect(book.destroy).not.toHaveBeenCalled()
      await vi.advanceTimersByTimeAsync(1)
      await done
      expect(book.destroy).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it('swallows destroy errors from epub.js internals', async () => {
    const book = fakeBook({ destroy: vi.fn(() => { throw new Error('boom') }) })
    await expect(destroyWhenSettled(null, book)).resolves.toBeUndefined()
  })
})
