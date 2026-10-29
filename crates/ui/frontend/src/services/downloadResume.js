// Helpers for resumable URL downloads in the Content Manager.
//
// The server marks a failed URL task `resumable` when it kept the partial
// file; `POST /api/download/:task_id/retry` then continues from
// `bytes_downloaded` using an HTTP Range request.

import { formatBytes, t } from '../i18n/index.js'

const RETRIABLE_STATUSES = ['failed', 'cancelled']
const ACTIVE_STATUSES = ['queued', 'downloading', 'validating', 'verifying', 'routing']
const SHA256_PATTERN = /^[0-9a-f]{64}$/i

const toBytes = (value) => {
  const number = Number(value)
  return Number.isFinite(number) && number > 0 ? number : 0
}

/** Human-readable byte count in the UI locale, e.g. `1.5 GB` / `1,5 GB`. */
export const formatByteCount = (bytes) => formatBytes(toBytes(bytes))

/** True for statuses where a worker is (or is about to be) running. */
export const isActiveDownloadStatus = (status) => ACTIVE_STATUSES.includes(String(status || '').toLowerCase())

/** A failed or cancelled URL task can be re-queued; local imports cannot. */
export const canRetryDownload = (task) => {
  if (!task?.source?.url) return false
  return RETRIABLE_STATUSES.includes(String(task.status || '').toLowerCase())
}

/** Bytes that a retry will keep, or 0 when it would start from scratch. */
export const resumableBytes = (task) => (task?.resumable ? toBytes(task.bytes_downloaded) : 0)

/** Button label: "Resume" when kept bytes exist, otherwise "Retry". */
export const retryActionLabel = (task) => (resumableBytes(task) > 0 ? t('content.downloads.resume') : t('content.downloads.retry'))

/**
 * Short status line about kept / resumed bytes, or `null` when there is
 * nothing to say.
 */
export const describeResumeState = (task, formatBytesFn = formatByteCount) => {
  if (!task) return null
  const kept = resumableBytes(task)
  const total = toBytes(task.total_bytes)
  const status = String(task.status || '').toLowerCase()

  if (canRetryDownload(task) && kept > 0) {
    return total > 0
      ? t('content.downloads.keptOf', { kept: formatBytesFn(kept), total: formatBytesFn(total) })
      : t('content.downloads.kept', { kept: formatBytesFn(kept) })
  }

  const resumedFrom = toBytes(task.resumed_from)
  if (resumedFrom > 0 && (isActiveDownloadStatus(status) || status === 'completed')) {
    return t('content.downloads.resumedFrom', { bytes: formatBytesFn(resumedFrom) })
  }

  return null
}

/**
 * Validate an optional SHA-256 from user input or the curated catalog.
 * Returns `{ value }` with the lowercase digest (or `null` when empty), or
 * `{ error }` when it is malformed.
 */
export const normalizeSha256Input = (input) => {
  const trimmed = String(input ?? '').trim()
  if (!trimmed) return { value: null }
  if (!SHA256_PATTERN.test(trimmed)) {
    return { error: t('content.errors.sha256') }
  }
  return { value: trimmed.toLowerCase() }
}
