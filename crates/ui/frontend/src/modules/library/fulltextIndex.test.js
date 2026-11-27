import { describe, expect, it } from 'vitest'
import {
  describeIndexEntry,
  isZimFilename,
  normalizeIndexStatus,
  shouldPollIndex
} from './fulltextIndex.js'

const payload = {
  enabled: true,
  auto_max_articles: 200000,
  min_free_mb: 1024,
  low_disk: false,
  archives: [
    { filename: 'wiki.zim', state: 'ready', articles: 120, indexed_articles: 118, percent: 100, auto: true, index_bytes: 4096 },
    { filename: 'big.zim', state: 'not_indexed', articles: 6000000, indexed_articles: 0, percent: 0, auto: false },
    { filename: 'mid.zim', state: 'indexing', percent: 42.4, indexed_articles: 1000, auto: true },
    { filename: 'weird.zim', state: 'exploded', percent: 250 },
    { state: 'ready' },
    null
  ]
}

describe('normalizeIndexStatus', () => {
  it('keeps valid entries and clamps values', () => {
    const status = normalizeIndexStatus(payload)
    expect(status.enabled).toBe(true)
    expect(status.lowDisk).toBe(false)
    expect(status.autoMaxArticles).toBe(200000)
    expect([...status.byFile.keys()]).toEqual(['wiki.zim', 'big.zim', 'mid.zim', 'weird.zim'])
    expect(status.byFile.get('weird.zim')).toMatchObject({ state: 'not_indexed', percent: 100 })
    expect(status.byFile.get('wiki.zim')).toMatchObject({ indexedArticles: 118, indexBytes: 4096, auto: true })
  })

  it('tolerates missing or malformed payloads', () => {
    for (const raw of [null, undefined, 'x', { archives: 'no' }]) {
      const status = normalizeIndexStatus(raw)
      expect(status.byFile.size).toBe(0)
      expect(status.enabled).toBe(true)
    }
    expect(normalizeIndexStatus({ enabled: false }).enabled).toBe(false)
  })
})

describe('describeIndexEntry', () => {
  const status = normalizeIndexStatus(payload)

  it('describes each state with the admin actions that make sense', () => {
    expect(describeIndexEntry(status.byFile.get('wiki.zim'))).toMatchObject({
      label: 'Full-text ready', tone: 'ok', canBuild: false, canDelete: true, title: '118 articles indexed'
    })
    expect(describeIndexEntry(status.byFile.get('mid.zim'))).toMatchObject({
      label: 'Indexing 42%', showProgress: true, percent: 42.4, canBuild: false, canDelete: true
    })
    expect(describeIndexEntry(status.byFile.get('big.zim'))).toMatchObject({ label: 'Not indexed', canBuild: true })
    expect(describeIndexEntry(status.byFile.get('big.zim')).title).toMatch(/manually/)
    expect(describeIndexEntry({ state: 'not_indexed', auto: true, percent: 0 }).label).toBe('Waiting to index')
    expect(describeIndexEntry({ state: 'failed', percent: 3, note: 'The archive could not be read' })).toMatchObject({
      tone: 'error', canBuild: true, title: 'The archive could not be read'
    })
    expect(describeIndexEntry({ state: 'paused', percent: 10 })).toMatchObject({ tone: 'warn', canBuild: true, showProgress: true })
    expect(describeIndexEntry({ state: 'excluded', percent: 0 })).toMatchObject({ canBuild: true, canDelete: false })
    expect(describeIndexEntry({ state: 'queued', percent: 0 })).toMatchObject({ label: 'Queued', canDelete: true })
    expect(describeIndexEntry(undefined)).toMatchObject({ label: 'Not indexed', canBuild: true })
  })

  it('offers no actions when the server has full-text off', () => {
    const info = describeIndexEntry(status.byFile.get('wiki.zim'), { enabled: false })
    expect(info).toMatchObject({ label: 'Full-text off', canBuild: false, canDelete: false })
  })
})

describe('polling and filenames', () => {
  it('polls only while work is pending', () => {
    expect(shouldPollIndex(normalizeIndexStatus(payload))).toBe(true)
    const idle = normalizeIndexStatus({ archives: [payload.archives[0], payload.archives[1]] })
    expect(shouldPollIndex(idle)).toBe(false)
    expect(shouldPollIndex(null)).toBe(false)
  })

  it('recognizes ZIM filenames', () => {
    expect(isZimFilename('wiki.ZIM')).toBe(true)
    expect(isZimFilename('book.epub')).toBe(false)
    expect(isZimFilename(null)).toBe(false)
  })
})
