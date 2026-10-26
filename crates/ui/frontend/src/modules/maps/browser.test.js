import { describe, expect, it, vi } from 'vitest'
import { copyText, downloadText, readFileText } from './browser.js'

describe('copyText', () => {
  it('uses the async clipboard API when available', async () => {
    const writeText = vi.fn(async () => {})
    expect(await copyText('59.33, 18.07', { nav: { clipboard: { writeText } }, doc: document })).toBe(true)
    expect(writeText).toHaveBeenCalledWith('59.33, 18.07')
  })

  it('falls back to execCommand on insecure origins', async () => {
    const execCommand = vi.fn(() => true)
    const doc = Object.assign(document, { execCommand })
    expect(await copyText('x', { nav: {}, doc })).toBe(true)
    expect(execCommand).toHaveBeenCalledWith('copy')
    expect(document.querySelectorAll('textarea')).toHaveLength(0)
  })

  it('falls back when the clipboard API rejects', async () => {
    const execCommand = vi.fn(() => false)
    const doc = Object.assign(document, { execCommand })
    const nav = { clipboard: { writeText: vi.fn(async () => { throw new Error('denied') }) } }
    expect(await copyText('x', { nav, doc })).toBe(false)
    expect(execCommand).toHaveBeenCalled()
  })
})

describe('downloadText', () => {
  it('creates and clicks a temporary link', () => {
    const createObjectURL = vi.fn(() => 'blob:fake')
    const revokeObjectURL = vi.fn()
    const clicks = []
    const origCreate = document.createElement.bind(document)
    const spy = vi.spyOn(document, 'createElement').mockImplementation((tag) => {
      const el = origCreate(tag)
      if (tag === 'a') el.click = () => clicks.push({ href: el.href, download: el.download })
      return el
    })
    downloadText('pois.gpx', '<gpx/>', 'application/gpx+xml', { doc: document, url: { createObjectURL, revokeObjectURL } })
    spy.mockRestore()
    expect(createObjectURL).toHaveBeenCalledTimes(1)
    expect(clicks).toEqual([{ href: 'blob:fake', download: 'pois.gpx' }])
    expect(document.querySelectorAll('a')).toHaveLength(0)
  })
})

describe('readFileText', () => {
  it('reads Blob/File text', async () => {
    expect(await readFileText(new Blob(['hello']))).toBe('hello')
  })
})
