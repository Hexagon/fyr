// End-to-end encryption for Fyr message channels.
//
// All channel cryptography lives in this file. It uses the audited pure-JS
// @noble libraries instead of WebCrypto because `crypto.subtle` is not
// available on plain-HTTP LAN origins (Fyr's normal deployment). Only
// `crypto.getRandomValues`, which works in insecure contexts, is needed.
//
// Spec (version 1) — keep in sync with DEVELOPER_MANUAL.md:
//   name'       = lowercase(trim(NFKC(name)))
//   passphrase' = NFKC(passphrase)
//   salt        = SHA-256("fyr-channel/v1" || server_salt[32] || utf8(name'))
//   master      = scrypt(utf8(passphrase'), salt, N=2^15, r=8, p=1, dkLen=32)
//   channel_id  = hex(HKDF-SHA256(master, salt="", info="fyr/v1/id",   16))
//   auth_key    =     HKDF-SHA256(master, salt="", info="fyr/v1/auth", 32)
//   enc_key     =     HKDF-SHA256(master, salt="", info="fyr/v1/enc",  32)
//   verifier    = hex(SHA-256(auth_key))          (stored by the server)
//   message     = XChaCha20-Poly1305(enc_key, nonce = random 24 bytes,
//                                    aad = utf8(channel_id),
//                                    plaintext = utf8(JSON payload, space-padded))
//   payload     = { v: 1, nick, device_tag, text, sent_at, [category], [lat, lng] }

import { scryptAsync } from '@noble/hashes/scrypt.js'
import { hkdf } from '@noble/hashes/hkdf.js'
import { sha256 } from '@noble/hashes/sha2.js'
import { bytesToHex, hexToBytes, utf8ToBytes } from '@noble/hashes/utils.js'
import { xchacha20poly1305 } from '@noble/ciphers/chacha.js'
import { localizedError } from '../../i18n/errors.js'

/** Versioned KDF parameters. Never change an existing entry: add a new one. */
export const KDF_VERSIONS = Object.freeze({
  1: Object.freeze({ N: 2 ** 15, r: 8, p: 1, dkLen: 32, label: 'fyr-channel/v1', infoPrefix: 'fyr/v1/' })
})
export const CURRENT_KDF_VERSION = 1
export const PAYLOAD_VERSION = 1
export const NONCE_BYTES = 24
export const MAX_CIPHERTEXT_BYTES = 8192
const TAG_BYTES = 16
const PAD_BLOCK = 64
const CATEGORIES = new Set(['info', 'need', 'offer', 'alert'])

const textDecoder = new TextDecoder('utf-8', { fatal: true })

export const normalizeChannelName = (name) => String(name ?? '').normalize('NFKC').trim().toLowerCase()
export const normalizePassphrase = (passphrase) => String(passphrase ?? '').normalize('NFKC')

const concatBytes = (...parts) => {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0))
  let offset = 0
  for (const part of parts) {
    out.set(part, offset)
    offset += part.length
  }
  return out
}

export const randomBytes = (length) => {
  const out = new Uint8Array(length)
  globalThis.crypto.getRandomValues(out)
  return out
}

export const toBase64 = (bytes) => {
  let binary = ''
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000))
  }
  return btoa(binary)
}

export const fromBase64 = (text) => {
  const binary = atob(String(text))
  const out = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i += 1) out[i] = binary.charCodeAt(i)
  return out
}

export const parseServerSalt = (hex) => {
  if (typeof hex !== 'string' || !/^[0-9a-f]{64}$/i.test(hex)) {
    throw new Error('Invalid server salt')
  }
  return hexToBytes(hex.toLowerCase())
}

/** SHA-256("fyr-channel/vN" || server_salt || utf8(normalized name)). */
export const channelSalt = (serverSaltHex, name, version = CURRENT_KDF_VERSION) => {
  const params = KDF_VERSIONS[version]
  if (!params) throw new Error(`Unknown KDF version ${version}`)
  return sha256(concatBytes(utf8ToBytes(params.label), parseServerSalt(serverSaltHex), utf8ToBytes(normalizeChannelName(name))))
}

/**
 * Derive channel id and keys from name + passphrase. Slow by design
 * (scrypt); `onProgress(0..1)` reports progress.
 *
 * @returns {Promise<{ version, name, channelId, authKey, encKey }>}
 */
export const deriveChannelKeys = async ({ name, passphrase, serverSalt, version = CURRENT_KDF_VERSION, onProgress, kdfOverride }) => {
  const params = kdfOverride || KDF_VERSIONS[version]
  if (!params) throw new Error(`Unknown KDF version ${version}`)
  const normalizedName = normalizeChannelName(name)
  if (!normalizedName) throw localizedError('messages.errors.nameRequired', 'Channel name is required')
  const pass = normalizePassphrase(passphrase)
  if (!pass) throw localizedError('messages.errors.passphraseRequired', 'Passphrase is required')

  const salt = channelSalt(serverSalt, normalizedName, version)
  const master = await scryptAsync(utf8ToBytes(pass), salt, {
    N: params.N,
    r: params.r,
    p: params.p,
    dkLen: params.dkLen,
    onProgress
  })
  const info = (label) => utf8ToBytes(`${KDF_VERSIONS[version].infoPrefix}${label}`)
  const result = {
    version,
    name: normalizedName,
    channelId: bytesToHex(hkdf(sha256, master, undefined, info('id'), 16)),
    authKey: hkdf(sha256, master, undefined, info('auth'), 32),
    encKey: hkdf(sha256, master, undefined, info('enc'), 32)
  }
  master.fill(0)
  return result
}

/** hex(SHA-256(auth_key)), registered with the server on channel creation. */
export const channelVerifier = (authKey) => bytesToHex(sha256(authKey))

export const keyToHex = (key) => bytesToHex(key)
export const keyFromHex = (hex) => {
  if (typeof hex !== 'string' || !/^[0-9a-f]{64}$/i.test(hex)) throw new Error('Invalid key')
  return hexToBytes(hex.toLowerCase())
}

const padPlaintext = (json) => {
  const bytes = utf8ToBytes(json)
  // Pad with JSON-insignificant spaces to hide exact message length.
  const target = Math.ceil((bytes.length + 1) / PAD_BLOCK) * PAD_BLOCK
  const limit = MAX_CIPHERTEXT_BYTES - TAG_BYTES
  const size = Math.min(target, Math.max(limit, bytes.length))
  const out = new Uint8Array(size).fill(0x20)
  out.set(bytes)
  return out
}

/** Build the canonical plaintext payload for an encrypted message. */
export const buildPayload = ({ nick, deviceTag, text, sentAt = Date.now(), category, lat, lng }) => {
  const payload = { v: PAYLOAD_VERSION, nick: String(nick), device_tag: String(deviceTag), text: String(text), sent_at: sentAt }
  if (category && CATEGORIES.has(category)) payload.category = category
  if (Number.isFinite(lat) && Number.isFinite(lng)) {
    payload.lat = lat
    payload.lng = lng
  }
  return payload
}

/**
 * Encrypt a payload object for `channelId`.
 * @returns {{ nonce: string, ciphertext: string }} base64 fields for the API
 */
export const encryptMessage = (encKey, channelId, payload, nonce = randomBytes(NONCE_BYTES)) => {
  const plaintext = padPlaintext(JSON.stringify(payload))
  if (plaintext.length + TAG_BYTES > MAX_CIPHERTEXT_BYTES) {
    throw localizedError('messages.errors.tooLong', 'Message is too long to encrypt')
  }
  const sealed = xchacha20poly1305(encKey, nonce, utf8ToBytes(channelId)).encrypt(plaintext)
  return { nonce: toBase64(nonce), ciphertext: toBase64(sealed) }
}

// Control characters (except newline/tab) and bidi overrides, which could
// disguise text. The server rejects these for public messages; encrypted
// payloads are cleaned here because the server cannot see them.
// eslint-disable-next-line no-control-regex
const UNSAFE_CHARS = /[\u0000-\u0008\u000B-\u001F\u007F-\u009F\u202A-\u202E\u2066-\u2069]/g
const cleanText = (value, max) => [...value.replace(/\r\n/g, '\n').replace(UNSAFE_CHARS, '')].slice(0, max).join('')

const validatePayload = (payload) => {
  if (!payload || typeof payload !== 'object' || payload.v !== PAYLOAD_VERSION) throw new Error('Unsupported payload')
  for (const field of ['nick', 'device_tag', 'text']) {
    if (typeof payload[field] !== 'string') throw new Error(`Invalid payload field ${field}`)
  }
  if (!Number.isFinite(payload.sent_at)) throw new Error('Invalid payload field sent_at')
  const clean = {
    v: payload.v,
    nick: cleanText(payload.nick.replace(/\n|\t/g, ' '), 40),
    device_tag: /^[0-9a-f]{4,16}$/.test(payload.device_tag) ? payload.device_tag : '0000',
    text: cleanText(payload.text, 2000),
    sent_at: payload.sent_at
  }
  if (CATEGORIES.has(payload.category)) clean.category = payload.category
  if (Number.isFinite(payload.lat) && Number.isFinite(payload.lng) && Math.abs(payload.lat) <= 90 && Math.abs(payload.lng) <= 180) {
    clean.lat = payload.lat
    clean.lng = payload.lng
  }
  return clean
}

/**
 * Decrypt and validate a stored message. Throws when the key is wrong, the
 * ciphertext or nonce was modified, or it belongs to another channel (AAD).
 */
export const decryptMessage = (encKey, channelId, { nonce, ciphertext }) => {
  const nonceBytes = fromBase64(nonce)
  if (nonceBytes.length !== NONCE_BYTES) throw new Error('Invalid nonce')
  const plaintext = xchacha20poly1305(encKey, nonceBytes, utf8ToBytes(channelId)).decrypt(fromBase64(ciphertext))
  return validatePayload(JSON.parse(textDecoder.decode(plaintext)))
}

// ---------------------------------------------------------------------------
// Encrypted export of remembered channel keys (move keys between devices).
//
// Spec (version 1):
//   file       = { format: "fyr-channel-keys", v: 1,
//                  kdf: { name: "scrypt", N, r, p }, salt: b64(16), nonce: b64(24),
//                  ciphertext: b64 }
//   key        = scrypt(utf8(NFKC(passphrase)), salt, N, r=8, p=1, dkLen=32)
//   aad        = utf8("fyr-channel-keys/v1|" + N + "|" + r + "|" + p + "|" + salt)
//   ciphertext = XChaCha20-Poly1305(key, nonce, aad, utf8(JSON { exported_at, channels }))
// The header is bound as AAD, so changing the KDF parameters or salt makes
// decryption fail just like a wrong passphrase or modified ciphertext.
// Keys are never exported without this encryption.
// ---------------------------------------------------------------------------

export const KEY_EXPORT_FORMAT = 'fyr-channel-keys'
export const KEY_EXPORT_VERSION = 1
export const KEY_EXPORT_MIN_PASSPHRASE = 8
const KEY_EXPORT_KDF = Object.freeze({ name: 'scrypt', N: 2 ** 15, r: 8, p: 1 })
const KEY_EXPORT_MAX_BYTES = 256 * 1024
const HEX32 = /^[0-9a-f]{32}$/
const HEX64 = /^[0-9a-f]{64}$/
const notAKeyFile = () => localizedError('messages.errors.notAKeyFile', 'Not a Fyr channel key file')
const wrongPassphrase = () => localizedError('messages.errors.wrongPassphrase', 'Wrong passphrase or damaged file')

const keyExportAad = ({ N, r, p }, saltB64) => utf8ToBytes(`fyr-channel-keys/v1|${N}|${r}|${p}|${saltB64}`)

// Bounded so a crafted file cannot make the importer allocate gigabytes.
const validKdf = (kdf) =>
  Boolean(kdf) && kdf.name === 'scrypt' && Number.isInteger(kdf.N) && kdf.N >= 2 ** 10 && kdf.N <= 2 ** 18 &&
  (kdf.N & (kdf.N - 1)) === 0 && kdf.r === 8 && kdf.p === 1

const cleanChannels = (list) =>
  (Array.isArray(list) ? list : [])
    .filter((c) => c && typeof c.name === 'string' && c.name.trim() && HEX32.test(c.channelId) && HEX64.test(c.authKey) && HEX64.test(c.encKey))
    .slice(0, 500)
    .map((c) => ({
      name: c.name.slice(0, 80),
      channelId: c.channelId,
      authKey: c.authKey,
      encKey: c.encKey,
      version: Number.isInteger(c.version) && KDF_VERSIONS[c.version] ? c.version : CURRENT_KDF_VERSION
    }))

/**
 * Encrypt remembered channels (`{ name, channelId, authKey, encKey, version }`
 * with hex keys) under a passphrase. Returns the JSON text of the file.
 */
export const exportChannelKeys = async (channels, passphrase, { kdf = KEY_EXPORT_KDF, onProgress, now = Date.now() } = {}) => {
  const pass = normalizePassphrase(passphrase)
  if ([...pass].length < KEY_EXPORT_MIN_PASSPHRASE) {
    throw localizedError('messages.errors.filePassphraseShort', `Use a passphrase of at least ${KEY_EXPORT_MIN_PASSPHRASE} characters`, { min: KEY_EXPORT_MIN_PASSPHRASE })
  }
  if (!validKdf(kdf)) throw new Error('Invalid KDF parameters')
  const list = cleanChannels(channels)
  if (!list.length) throw localizedError('messages.errors.nothingToExport', 'There are no remembered channels to export')
  const salt = randomBytes(16)
  const saltB64 = toBase64(salt)
  const key = await scryptAsync(utf8ToBytes(pass), salt, { N: kdf.N, r: kdf.r, p: kdf.p, dkLen: 32, onProgress })
  const nonce = randomBytes(NONCE_BYTES)
  const plaintext = utf8ToBytes(JSON.stringify({ exported_at: now, channels: list }))
  const sealed = xchacha20poly1305(key, nonce, keyExportAad(kdf, saltB64)).encrypt(plaintext)
  key.fill(0)
  return JSON.stringify(
    {
      format: KEY_EXPORT_FORMAT,
      v: KEY_EXPORT_VERSION,
      kdf: { name: 'scrypt', N: kdf.N, r: kdf.r, p: kdf.p },
      salt: saltB64,
      nonce: toBase64(nonce),
      ciphertext: toBase64(sealed)
    },
    null,
    2
  )
}

/**
 * Decrypt a key export file. Throws "Wrong passphrase or damaged file" when
 * the passphrase is wrong or any part of the file was modified.
 * @returns {Promise<{ exportedAt: number|null, channels: Array }>}
 */
export const importChannelKeys = async (text, passphrase, { onProgress } = {}) => {
  if (typeof text !== 'string' || text.length > KEY_EXPORT_MAX_BYTES) throw notAKeyFile()
  let file
  try {
    file = JSON.parse(text)
  } catch {
    throw notAKeyFile()
  }
  if (!file || file.format !== KEY_EXPORT_FORMAT) throw notAKeyFile()
  if (file.v !== KEY_EXPORT_VERSION) throw localizedError('messages.errors.keyFileVersion', `Unsupported key file version ${file.v}`, { version: String(file.v) })
  if (!validKdf(file.kdf) || typeof file.salt !== 'string' || typeof file.nonce !== 'string' || typeof file.ciphertext !== 'string') {
    throw notAKeyFile()
  }
  let salt, nonce, ciphertext
  try {
    salt = fromBase64(file.salt)
    nonce = fromBase64(file.nonce)
    ciphertext = fromBase64(file.ciphertext)
  } catch {
    throw notAKeyFile()
  }
  if (salt.length !== 16 || nonce.length !== NONCE_BYTES || ciphertext.length < TAG_BYTES) throw notAKeyFile()
  const key = await scryptAsync(utf8ToBytes(normalizePassphrase(passphrase)), salt, { N: file.kdf.N, r: 8, p: 1, dkLen: 32, onProgress })
  let plaintext
  try {
    plaintext = xchacha20poly1305(key, nonce, keyExportAad(file.kdf, file.salt)).decrypt(ciphertext)
  } catch {
    throw wrongPassphrase()
  } finally {
    key.fill(0)
  }
  let payload
  try {
    payload = JSON.parse(textDecoder.decode(plaintext))
  } catch {
    throw wrongPassphrase()
  }
  return {
    exportedAt: Number.isFinite(payload?.exported_at) ? payload.exported_at : null,
    channels: cleanChannels(payload?.channels)
  }
}
