import { describe, expect, it } from 'vitest'
import { activeAlerts, dropExpired, latestId, mergeMessages, toViewMessage } from './model.js'
import { buildPayload, deriveChannelKeys, encryptMessage } from './crypto.js'

const SALT = '11'.repeat(32)

describe('message model', () => {
  it('maps public rows and marks own messages by device tag', () => {
    const row = { id: 3, channel_id: 'public', created_at: 1, expires_at: 9, pinned: false, nick: 'A', device_tag: 'abcdef', text: 'hi', category: 'alert' }
    expect(toViewMessage(row, { deviceTag: 'abcdef' })).toMatchObject({ id: 3, text: 'hi', own: true, encrypted: false, category: 'alert' })
    expect(toViewMessage(row, { deviceTag: '123456' }).own).toBe(false)
  })

  it('decrypts encrypted rows and degrades gracefully on failure', async () => {
    const keys = await deriveChannelKeys({ name: 'c', passphrase: 'p', serverSalt: SALT, kdfOverride: { N: 16, r: 8, p: 1, dkLen: 32 } })
    const sealed = encryptMessage(keys.encKey, keys.channelId, buildPayload({ nick: 'B', deviceTag: 'beef01', text: 'secret', sentAt: 5 }))
    const row = { id: 1, channel_id: keys.channelId, created_at: 1, pinned: false, ...sealed }
    expect(toViewMessage(row, { encKey: keys.encKey, deviceTag: 'beef01' })).toMatchObject({ text: 'secret', nick: 'B', own: true, encrypted: true })
    const broken = toViewMessage({ ...row, ciphertext: sealed.ciphertext.slice(4) }, { encKey: keys.encKey, deviceTag: 'beef01' })
    expect(broken).toMatchObject({ undecryptable: true, own: false, text: '' })
  })

  it('merges, de-duplicates, caps and filters', () => {
    const merged = mergeMessages([{ id: 1 }, { id: 3 }], [{ id: 2 }, { id: 3, x: 1 }], 2)
    expect(merged).toEqual([{ id: 2 }, { id: 3, x: 1 }])
    expect(latestId(merged)).toBe(3)
    const now = 100
    const list = [
      { id: 1, expires_at: 50, pinned: false, category: 'alert' },
      { id: 2, expires_at: 50, pinned: true, category: 'alert' },
      { id: 3, expires_at: 200, pinned: false, category: 'alert' },
      { id: 4, expires_at: 200, pinned: false, category: 'info' }
    ]
    expect(dropExpired(list, now).map((m) => m.id)).toEqual([2, 3, 4])
    expect(activeAlerts(list, new Set([3]), now).map((m) => m.id)).toEqual([2])
  })
})
