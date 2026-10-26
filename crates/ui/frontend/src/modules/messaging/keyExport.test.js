import { describe, expect, it } from 'vitest'
import { exportChannelKeys, fromBase64, importChannelKeys, toBase64 } from './crypto.js'
import { createMessagingStorage } from './storage.js'

// Smallest N the importer accepts; keeps tests fast.
const FAST = { name: 'scrypt', N: 2 ** 10, r: 8, p: 1 }
const PASS = 'correct horse battery staple'

const channel = (i) => ({
  name: `camp ${i}`,
  channelId: String(i).padStart(2, '0').repeat(16),
  authKey: 'a'.repeat(63) + String(i % 10),
  encKey: 'b'.repeat(63) + String(i % 10),
  version: 1
})

const memoryBacking = () => {
  const map = new Map()
  return { getItem: (k) => (map.has(k) ? map.get(k) : null), setItem: (k, v) => map.set(k, String(v)), removeItem: (k) => map.delete(k) }
}

describe('channel key export', () => {
  it('round-trips remembered channels through an encrypted file', async () => {
    const source = createMessagingStorage(memoryBacking())
    source.rememberChannel(channel(1))
    source.rememberChannel(channel(2))

    const text = await exportChannelKeys(source.channels(), PASS, { kdf: FAST, now: 1234 })
    // No key material in the clear.
    for (const c of source.channels()) {
      expect(text).not.toContain(c.authKey)
      expect(text).not.toContain(c.encKey)
      expect(text).not.toContain(c.channelId)
      expect(text).not.toContain(c.name)
    }
    const file = JSON.parse(text)
    expect(file).toMatchObject({ format: 'fyr-channel-keys', v: 1, kdf: FAST })

    const imported = await importChannelKeys(text, PASS)
    expect(imported.exportedAt).toBe(1234)
    expect(imported.channels).toEqual(source.channels())

    const target = createMessagingStorage(memoryBacking())
    target.rememberChannel(channel(2))
    for (const c of imported.channels) target.rememberChannel(c)
    expect(target.channels().map((c) => c.channelId).sort()).toEqual([channel(1).channelId, channel(2).channelId].sort())
  })

  it('uses a fresh salt and nonce every time', async () => {
    const a = JSON.parse(await exportChannelKeys([channel(1)], PASS, { kdf: FAST }))
    const b = JSON.parse(await exportChannelKeys([channel(1)], PASS, { kdf: FAST }))
    expect(a.salt).not.toBe(b.salt)
    expect(a.nonce).not.toBe(b.nonce)
    expect(a.ciphertext).not.toBe(b.ciphertext)
  })

  it('refuses short passphrases and empty exports', async () => {
    await expect(exportChannelKeys([channel(1)], 'short', { kdf: FAST })).rejects.toThrow(/at least 8/)
    await expect(exportChannelKeys([], PASS, { kdf: FAST })).rejects.toThrow(/no remembered channels/)
  })

  it('rejects a wrong passphrase', async () => {
    const text = await exportChannelKeys([channel(1)], PASS, { kdf: FAST })
    await expect(importChannelKeys(text, 'Correct horse battery staple')).rejects.toThrow('Wrong passphrase or damaged file')
  })

  it('rejects tampered files', async () => {
    const text = await exportChannelKeys([channel(1)], PASS, { kdf: FAST })
    const file = JSON.parse(text)

    const flip = (b64) => {
      const bytes = fromBase64(b64)
      bytes[bytes.length - 1] ^= 1
      return toBase64(bytes)
    }
    const variants = [
      { ...file, ciphertext: flip(file.ciphertext) },
      { ...file, nonce: flip(file.nonce) },
      { ...file, salt: flip(file.salt) },
      { ...file, kdf: { ...file.kdf, N: 2 ** 11 } }
    ]
    for (const variant of variants) {
      await expect(importChannelKeys(JSON.stringify(variant), PASS)).rejects.toThrow('Wrong passphrase or damaged file')
    }

    for (const bad of [
      'not json',
      JSON.stringify({ ...file, format: 'other' }),
      JSON.stringify({ ...file, kdf: { ...file.kdf, N: 2 ** 24 } }),
      JSON.stringify({ ...file, kdf: { ...file.kdf, r: 64 } }),
      JSON.stringify({ ...file, nonce: toBase64(new Uint8Array(12)) }),
      JSON.stringify({ ...file, ciphertext: 'AAAA' })
    ]) {
      await expect(importChannelKeys(bad, PASS)).rejects.toThrow(/Not a Fyr channel key file/)
    }
    await expect(importChannelKeys(JSON.stringify({ ...file, v: 2 }), PASS)).rejects.toThrow(/Unsupported/)
  })
})
