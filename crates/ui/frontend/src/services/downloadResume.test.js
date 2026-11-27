import { describe, expect, it } from 'vitest'
import {
  canRetryDownload,
  describeResumeState,
  formatByteCount,
  isActiveDownloadStatus,
  normalizeSha256Input,
  resumableBytes,
  retryActionLabel
} from './downloadResume.js'

const urlTask = (overrides = {}) => ({
  id: 't1',
  source: { type: 'Url', url: 'https://example.com/atlas.pmtiles' },
  status: 'failed',
  bytes_downloaded: 0,
  total_bytes: null,
  resumable: false,
  resumed_from: null,
  ...overrides
})

describe('canRetryDownload', () => {
  it('allows failed and cancelled URL tasks', () => {
    expect(canRetryDownload(urlTask())).toBe(true)
    expect(canRetryDownload(urlTask({ status: 'Cancelled' }))).toBe(true)
  })

  it('rejects active, completed and local-import tasks', () => {
    expect(canRetryDownload(urlTask({ status: 'downloading' }))).toBe(false)
    expect(canRetryDownload(urlTask({ status: 'completed' }))).toBe(false)
    expect(canRetryDownload({ status: 'failed', source: { type: 'LocalFile', path: 'x.epub' } })).toBe(false)
    expect(canRetryDownload(null)).toBe(false)
  })
})

describe('resume labels', () => {
  it('says Resume only when bytes were kept', () => {
    expect(retryActionLabel(urlTask({ resumable: true, bytes_downloaded: 2048 }))).toBe('Resume')
    expect(retryActionLabel(urlTask({ resumable: false, bytes_downloaded: 2048 }))).toBe('Retry')
    expect(retryActionLabel(urlTask({ resumable: true, bytes_downloaded: 0 }))).toBe('Retry')
    expect(resumableBytes(urlTask({ resumable: true, bytes_downloaded: '512' }))).toBe(512)
  })

  it('describes kept bytes for resumable failures', () => {
    expect(describeResumeState(urlTask({ resumable: true, bytes_downloaded: 1024, total_bytes: 4096 })))
      .toBe('1 KB of 4 KB kept – resume continues from there')
    expect(describeResumeState(urlTask({ resumable: true, bytes_downloaded: 1536 })))
      .toBe('1.5 KB kept – resume continues from there')
  })

  it('describes the resume offset of active and completed transfers', () => {
    expect(describeResumeState(urlTask({ status: 'downloading', resumed_from: 1048576 }))).toBe('Resumed from 1 MB')
    expect(describeResumeState(urlTask({ status: 'completed', resumed_from: 1024 }))).toBe('Resumed from 1 KB')
  })

  it('returns null when there is nothing to report', () => {
    expect(describeResumeState(urlTask())).toBeNull()
    expect(describeResumeState(urlTask({ status: 'downloading' }))).toBeNull()
    expect(describeResumeState(undefined)).toBeNull()
  })

  it('uses a custom formatter', () => {
    const text = describeResumeState(urlTask({ resumable: true, bytes_downloaded: 10 }), (n) => `${n}b`)
    expect(text).toBe('10b kept – resume continues from there')
  })
})

describe('helpers', () => {
  it('formats byte counts', () => {
    expect(formatByteCount(0)).toBe('0 B')
    expect(formatByteCount(3 * 1024 ** 3)).toBe('3 GB')
  })

  it('treats verifying as active', () => {
    expect(isActiveDownloadStatus('verifying')).toBe(true)
    expect(isActiveDownloadStatus('failed')).toBe(false)
  })

  it('validates optional SHA-256 input', () => {
    expect(normalizeSha256Input('')).toEqual({ value: null })
    expect(normalizeSha256Input(undefined)).toEqual({ value: null })
    expect(normalizeSha256Input(` ${'AB'.repeat(32)} `)).toEqual({ value: 'ab'.repeat(32) })
    expect(normalizeSha256Input('xyz').error).toMatch(/64 hexadecimal/)
  })
})
