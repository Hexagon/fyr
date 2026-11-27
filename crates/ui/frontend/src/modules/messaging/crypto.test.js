import { describe, expect, it } from 'vitest'
import { xchacha20poly1305 } from '@noble/ciphers/chacha.js'
import { scrypt } from '@noble/hashes/scrypt.js'
import { bytesToHex, hexToBytes, utf8ToBytes } from '@noble/hashes/utils.js'
import {
  KDF_VERSIONS,
  buildPayload,
  channelSalt,
  channelVerifier,
  decryptMessage,
  deriveChannelKeys,
  encryptMessage,
  fromBase64,
  keyFromHex,
  keyToHex,
  normalizeChannelName,
  normalizePassphrase,
  toBase64
} from './crypto.js'

const SERVER_SALT = '00'.repeat(31) + '01'
// Fast parameters for behavioural tests; the KAT below uses the real ones.
const FAST = { N: 2 ** 4, r: 8, p: 1, dkLen: 32 }
const fastKeys = (name, passphrase, serverSalt = SERVER_SALT) =>
  deriveChannelKeys({ name, passphrase, serverSalt, kdfOverride: FAST })

describe('primitives (known-answer tests)', () => {
  it('scrypt matches RFC 7914 test vector 2', () => {
    const out = scrypt('password', 'NaCl', { N: 1024, r: 8, p: 16, dkLen: 64 })
    expect(bytesToHex(out)).toBe(
      'fdbabe1c9d3472007856e7190d01e9fe7c6ad7cbc8237830e77376634b3731622eaf30d92e22a3886ff109279d9830dac727afb94a83ee6d8360cbdfa2cc0640'
    )
  })

  it('XChaCha20-Poly1305 matches draft-irtf-cfrg-xchacha A.3.1', () => {
    const key = hexToBytes('808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f')
    const nonce = hexToBytes('404142434445464748494a4b4c4d4e4f5051525354555657')
    const aad = hexToBytes('50515253c0c1c2c3c4c5c6c7')
    const pt = utf8ToBytes("Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.")
    const out = bytesToHex(xchacha20poly1305(key, nonce, aad).encrypt(pt))
    expect(out.startsWith('bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb')).toBe(true)
    expect(out.endsWith('c0875924c1c7987947deafd8780acf49')).toBe(true)
  })
})

describe('channel key derivation', () => {
  it('uses the versioned v1 parameters', () => {
    expect(KDF_VERSIONS[1]).toMatchObject({ N: 32768, r: 8, p: 1, dkLen: 32 })
  })

  it('matches the v1 known answer (cross-checked with Python hashlib)', async () => {
    const keys = await deriveChannelKeys({
      name: '  Camp North ',
      passphrase: 'correct horse battery staple',
      serverSalt: SERVER_SALT
    })
    expect(keys.name).toBe('camp north')
    expect(keys.channelId).toBe('39e96429af95b8633a030e5d556e2e53')
    expect(bytesToHex(keys.authKey)).toBe('b80dfed2ba708db21d2fd4405d79973fb510f8f084db5d458a269ea6f81aac14')
    expect(bytesToHex(keys.encKey)).toBe('bd16f2d300f62fab8cd32c5b84b10d5a68aafb2f83d50f825930595fbd8317d0')
    expect(channelVerifier(keys.authKey)).toBe('e6fb9c451a9da43694600a6a367cfd725118fdc909182096401cb337e99815b3')
  }, 20000)

  it('normalizes channel names (NFKC, trim, lowercase) and passphrases (NFKC only)', async () => {
    expect(normalizeChannelName('  Ｃａｍｐ North ')).toBe('camp north')
    expect(normalizeChannelName('Café')).toBe('café')
    expect(normalizePassphrase('Café')).toBe('Café')
    expect(normalizePassphrase(' Pass ')).toBe(' Pass ')

    const composed = await fastKeys('Café', 'Pässword')
    const decomposed = await fastKeys('CAFÉ ', 'Pässword')
    expect(decomposed.channelId).toBe(composed.channelId)
    expect(bytesToHex(decomposed.encKey)).toBe(bytesToHex(composed.encKey))

    // Passphrase case matters.
    const upper = await fastKeys('Café', 'PÄSSWORD')
    expect(upper.channelId).not.toBe(composed.channelId)
  })

  it('separates id, auth and enc keys and depends on every input', async () => {
    const base = await fastKeys('camp', 'secret one')
    expect(base.channelId).toHaveLength(32)
    expect(bytesToHex(base.authKey)).not.toBe(bytesToHex(base.encKey))
    expect((await fastKeys('camp2', 'secret one')).channelId).not.toBe(base.channelId)
    expect((await fastKeys('camp', 'secret two')).channelId).not.toBe(base.channelId)
    expect((await fastKeys('camp', 'secret one', 'ff'.repeat(32))).channelId).not.toBe(base.channelId)
  })

  it('rejects empty inputs and bad salts', async () => {
    await expect(fastKeys('  ', 'x')).rejects.toThrow()
    await expect(fastKeys('camp', '')).rejects.toThrow()
    await expect(fastKeys('camp', 'x', 'nothex')).rejects.toThrow()
    expect(() => channelSalt(SERVER_SALT, 'x', 99)).toThrow()
  })
})

describe('message encryption', () => {
  const payload = buildPayload({ nick: 'Ada', deviceTag: 'a1b2c3', text: 'Meet at the bridge 🌉', sentAt: 1700000000000, category: 'need', lat: 59.3, lng: 18.06 })

  it('round-trips payloads including category and location', async () => {
    const keys = await fastKeys('camp', 'pass')
    const sealed = encryptMessage(keys.encKey, keys.channelId, payload)
    expect(fromBase64(sealed.nonce)).toHaveLength(24)
    expect(decryptMessage(keys.encKey, keys.channelId, sealed)).toEqual(payload)
  })

  it('uses a fresh nonce per message and pads to hide exact length', async () => {
    const keys = await fastKeys('camp', 'pass')
    const a = encryptMessage(keys.encKey, keys.channelId, buildPayload({ nick: 'a', deviceTag: 'abcd', text: 'x', sentAt: 1 }))
    const b = encryptMessage(keys.encKey, keys.channelId, buildPayload({ nick: 'a', deviceTag: 'abcd', text: 'xyz', sentAt: 1 }))
    expect(a.nonce).not.toBe(b.nonce)
    expect(fromBase64(a.ciphertext).length).toBe(fromBase64(b.ciphertext).length)
    expect((fromBase64(a.ciphertext).length - 16) % 64).toBe(0)
  })

  it('detects tampering of ciphertext, tag and nonce', async () => {
    const keys = await fastKeys('camp', 'pass')
    const sealed = encryptMessage(keys.encKey, keys.channelId, payload)
    const ct = fromBase64(sealed.ciphertext)
    for (const index of [0, Math.floor(ct.length / 2), ct.length - 1]) {
      const copy = ct.slice()
      copy[index] ^= 1
      expect(() => decryptMessage(keys.encKey, keys.channelId, { ...sealed, ciphertext: toBase64(copy) })).toThrow()
    }
    const nonce = fromBase64(sealed.nonce)
    nonce[0] ^= 1
    expect(() => decryptMessage(keys.encKey, keys.channelId, { ...sealed, nonce: toBase64(nonce) })).toThrow()
    expect(() => decryptMessage(keys.encKey, keys.channelId, { ...sealed, ciphertext: toBase64(ct.slice(0, 10)) })).toThrow()
  })

  it('fails with a wrong passphrase', async () => {
    const keys = await fastKeys('camp', 'pass')
    const wrong = await fastKeys('camp', 'pass!')
    const sealed = encryptMessage(keys.encKey, keys.channelId, payload)
    expect(() => decryptMessage(wrong.encKey, keys.channelId, sealed)).toThrow()
  })

  it('binds ciphertext to the channel id (AAD)', async () => {
    const keys = await fastKeys('camp', 'pass')
    const sealed = encryptMessage(keys.encKey, keys.channelId, payload)
    expect(() => decryptMessage(keys.encKey, 'f'.repeat(32), sealed)).toThrow()
  })

  it('rejects oversize messages and sanitizes decrypted payloads', async () => {
    const keys = await fastKeys('camp', 'pass')
    expect(() =>
      encryptMessage(keys.encKey, keys.channelId, buildPayload({ nick: 'a', deviceTag: 'abcd', text: '😀'.repeat(2100), sentAt: 1 }))
    ).toThrow(/too long/)

    const hostile = { v: 1, nick: 'x'.repeat(100), device_tag: '<img onerror=1>', text: 'hi', sent_at: 5, category: 'evil', lat: 999, lng: 1 }
    const sealed = encryptMessage(keys.encKey, keys.channelId, hostile)
    const clean = decryptMessage(keys.encKey, keys.channelId, sealed)
    expect(clean.nick).toHaveLength(40)
    expect(clean.device_tag).toBe('0000')
    expect(clean.category).toBeUndefined()
    expect(clean.lat).toBeUndefined()

    const sneaky = encryptMessage(keys.encKey, keys.channelId, { v: 1, nick: 'a\u202Eb\nc', device_tag: 'abcd', text: 'x\u0007y\u2066z\r\nok', sent_at: 1 })
    expect(decryptMessage(keys.encKey, keys.channelId, sneaky)).toMatchObject({ nick: 'ab c', text: 'xyz\nok' })

    const bad = encryptMessage(keys.encKey, keys.channelId, { v: 2, text: 'x' })
    expect(() => decryptMessage(keys.encKey, keys.channelId, bad)).toThrow()
  })

  it('hex key helpers round-trip and validate', () => {
    const key = new Uint8Array(32).fill(7)
    expect(keyFromHex(keyToHex(key))).toEqual(key)
    expect(() => keyFromHex('abc')).toThrow()
  })
})
