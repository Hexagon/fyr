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
  if (!normalizedName) throw new Error('Channel name is required')
  const pass = normalizePassphrase(passphrase)
  if (!pass) throw new Error('Passphrase is required')

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
    throw new Error('Message is too long to encrypt')
  }
  const sealed = xchacha20poly1305(encKey, nonce, utf8ToBytes(channelId)).encrypt(plaintext)
  return { nonce: toBase64(nonce), ciphertext: toBase64(sealed) }
}

const validatePayload = (payload) => {
  if (!payload || typeof payload !== 'object' || payload.v !== PAYLOAD_VERSION) throw new Error('Unsupported payload')
  for (const field of ['nick', 'device_tag', 'text']) {
    if (typeof payload[field] !== 'string') throw new Error(`Invalid payload field ${field}`)
  }
  if (!Number.isFinite(payload.sent_at)) throw new Error('Invalid payload field sent_at')
  const clean = {
    v: payload.v,
    nick: payload.nick.slice(0, 40),
    device_tag: /^[0-9a-f]{4,16}$/.test(payload.device_tag) ? payload.device_tag : '0000',
    text: payload.text.slice(0, 4000),
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
