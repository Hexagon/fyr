<template>
  <div class="tools-page">
    <div class="tools-layout">
      <aside class="tools-sidebar" :class="{ collapsed: sidebarCollapsed }">
        <div class="sidebar-header">
          <h3>Tools</h3>
          <button class="icon-btn" @click="toggleSidebar" :title="sidebarCollapsed ? 'Expand panel' : 'Collapse panel'">
            {{ sidebarCollapsed ? '»' : '«' }}
          </button>
        </div>

        <div v-if="!sidebarCollapsed" class="sidebar-content">
          <div class="sidebar-section">
            <h4 class="sidebar-section-title">Unit Converters</h4>
            <a
              v-for="cat in converterCategories"
              :key="cat.id"
              :href="'#' + cat.id"
              class="sidebar-item"
              :class="{ active: activeConverter === cat.id }"
              @click.prevent="scrollToConverter(cat.id)"
            >
              {{ cat.label }}
            </a>
          </div>

          <div class="sidebar-section">
            <h4 class="sidebar-section-title">Encryption & Ciphers</h4>
            <button
              v-for="tool in cipherTools"
              :key="tool.id"
              class="sidebar-item"
              :class="{ active: activeCipher === tool.id }"
              @click="selectCipher(tool.id)"
            >
              {{ tool.label }}
            </button>
          </div>
        </div>
      </aside>

      <section class="tools-panel">
        <!-- Unit Converters Panel -->
        <template v-if="activeTab === 'converters'">
          <div class="panel-header">
            <h2>Unit Converters</h2>
          </div>

          <div
            v-for="cat in converterCategories"
            :key="cat.id"
            :id="cat.id"
            class="converter-card"
          >
            <div class="card-header">
              <h3>{{ cat.label }}</h3>
            </div>
            <div class="converter-body">
              <div class="converter-input-row">
                <input
                  v-model.number="converters[cat.id].value"
                  type="number"
                  step="any"
                  placeholder="Enter value"
                  class="tool-input"
                  @input="convertCurrent(cat.id)"
                />
                <select v-model="converters[cat.id].from" class="tool-select" @change="convertCurrent(cat.id)">
                  <option v-for="u in getUnits(cat.id)" :key="u" :value="u">{{ u }}</option>
                </select>
                <span class="arrow">→</span>
                <select v-model="converters[cat.id].to" class="tool-select" @change="convertCurrent(cat.id)">
                  <option v-for="u in getUnits(cat.id)" :key="u" :value="u">{{ u }}</option>
                </select>
              </div>
              <div class="converter-result-row">
                <div class="tool-result" v-if="converters[cat.id].result !== null">
                  <span class="result-value">{{ formatNumber(converters[cat.id].result) }}</span>
                </div>
              </div>
            </div>
          </div>
        </template>

        <!-- Cipher Panel -->
        <template v-if="activeTab === 'ciphers'">
          <div class="panel-header">
            <h2>{{ currentCipherLabel }}</h2>
          </div>

          <!-- AES -->
          <div v-if="activeCipher === 'aes'" class="cipher-card">
            <div class="card-header">
              <h3>AES-256-CBC</h3>
            </div>
            <div class="cipher-body">
              <div class="cipher-input-row">
                <label class="tool-label">
                  Mode
                  <select v-model="ciphers.aes.mode" class="tool-select">
                    <option value="encrypt">Encrypt</option>
                    <option value="decrypt">Decrypt</option>
                  </select>
                </label>
                <label class="tool-label">
                  Password
                  <input v-model="ciphers.aes.password" type="text" class="tool-input" placeholder="Enter password" />
                </label>
                <label class="tool-label tool-label-wide">
                  {{ ciphers.aes.mode === 'encrypt' ? 'Plaintext' : 'Ciphertext (hex)' }}
                  <textarea
                    v-model="ciphers.aes.text"
                    class="tool-textarea"
                    rows="4"
                    :placeholder="ciphers.aes.mode === 'encrypt' ? 'Text to encrypt' : 'Hex string to decrypt'"
                  ></textarea>
                </label>
                <div class="cipher-actions">
                  <button class="btn btn-primary" @click="handleAes" :disabled="cipherWorking">
                    {{ ciphers.aes.mode === 'encrypt' ? 'Encrypt' : 'Decrypt' }}
                  </button>
                </div>
              </div>
              <div class="cipher-result-row">
                <div class="tool-result" v-if="ciphers.aes.result !== null">
                  <span class="result-label">Result:</span>
                  <code class="result-code">{{ ciphers.aes.result }}</code>
                </div>
                <p v-if="ciphers.aes.error" class="cipher-error">{{ ciphers.aes.error }}</p>
              </div>
            </div>
          </div>

          <!-- Base64 -->
          <div v-if="activeCipher === 'base64'" class="cipher-card">
            <div class="card-header">
              <h3>Base64</h3>
            </div>
            <div class="cipher-body">
              <div class="cipher-input-row">
                <label class="tool-label">
                  Mode
                  <select v-model="ciphers.base64.mode" class="tool-select">
                    <option value="encode">Encode</option>
                    <option value="decode">Decode</option>
                  </select>
                </label>
                <label class="tool-label tool-label-wide">
                  {{ ciphers.base64.mode === 'encode' ? 'Plaintext' : 'Base64 string' }}
                  <textarea
                    v-model="ciphers.base64.text"
                    class="tool-textarea"
                    rows="4"
                    :placeholder="ciphers.base64.mode === 'encode' ? 'Text to encode' : 'Base64 string to decode'"
                    @input="handleBase64"
                  ></textarea>
                </label>
              </div>
              <div class="cipher-result-row">
                <div class="tool-result" v-if="ciphers.base64.result !== null">
                  <span class="result-label">Result:</span>
                  <code class="result-code">{{ ciphers.base64.result }}</code>
                </div>
                <p v-if="ciphers.base64.error" class="cipher-error">{{ ciphers.base64.error }}</p>
              </div>
            </div>
          </div>

          <!-- ROT13 -->
          <div v-if="activeCipher === 'rot13'" class="cipher-card">
            <div class="card-header">
              <h3>ROT13</h3>
            </div>
            <div class="cipher-body">
              <div class="cipher-input-row">
                <label class="tool-label tool-label-wide">
                  Input
                  <textarea
                    v-model="ciphers.rot13.text"
                    class="tool-textarea"
                    rows="4"
                    placeholder="Text to apply ROT13"
                    @input="handleRot13"
                  ></textarea>
                </label>
              </div>
              <div class="cipher-result-row">
                <div class="tool-result" v-if="ciphers.rot13.result !== null">
                  <span class="result-label">Result:</span>
                  <code class="result-code">{{ ciphers.rot13.result }}</code>
                </div>
              </div>
            </div>
          </div>

          <!-- Hash / Checksum -->
          <div v-if="activeCipher === 'hash'" class="cipher-card">
            <div class="card-header">
              <h3>Hash / Checksum</h3>
            </div>
            <div class="cipher-body">
              <div class="cipher-input-row">
                <label class="tool-label">
                  Algorithm
                  <select v-model="ciphers.hash.algo" class="tool-select" @change="handleHash">
                    <option value="sha256">SHA-256</option>
                    <option value="sha512">SHA-512</option>
                    <option value="sha1">SHA-1</option>
                    <option value="md5">MD5</option>
                  </select>
                </label>
                <label class="tool-label tool-label-wide">
                  Input
                  <textarea
                    v-model="ciphers.hash.text"
                    class="tool-textarea"
                    rows="4"
                    placeholder="Text to hash"
                    @input="handleHash"
                  ></textarea>
                </label>
              </div>
              <div class="cipher-result-row">
                <div class="tool-result" v-if="ciphers.hash.result !== null">
                  <span class="result-label">Hash:</span>
                  <code class="result-code">{{ ciphers.hash.result }}</code>
                </div>
              </div>
            </div>
          </div>
        </template>
      </section>
    </div>
  </div>
</template>

<script setup>
import { computed, reactive, ref } from 'vue'

const sidebarCollapsed = ref(false)
const activeTab = ref('converters')
const activeConverter = ref('length')
const activeCipher = ref('aes')
const cipherWorking = ref(false)

const converterCategories = [
  { id: 'length', label: 'Length' },
  { id: 'mass', label: 'Mass' },
  { id: 'temperature', label: 'Temperature' },
  { id: 'area', label: 'Area' },
  { id: 'volume', label: 'Volume' },
  { id: 'speed', label: 'Speed' },
  { id: 'data', label: 'Data' },
  { id: 'angle', label: 'Angle' }
]

const cipherTools = [
  { id: 'aes', label: 'AES-256-CBC' },
  { id: 'base64', label: 'Base64' },
  { id: 'rot13', label: 'ROT13' },
  { id: 'hash', label: 'Hash / Checksum' }
]

const currentCipherLabel = computed(() => {
  const tool = cipherTools.find(t => t.id === activeCipher.value)
  return tool ? tool.label : 'Cipher'
})

const toggleSidebar = () => {
  sidebarCollapsed.value = !sidebarCollapsed.value
}

const scrollToConverter = (id) => {
  activeTab.value = 'converters'
  activeConverter.value = id
  const el = document.getElementById(id)
  if (el) {
    el.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }
}

const selectCipher = (id) => {
  activeTab.value = 'ciphers'
  activeCipher.value = id
}

// --- Unit definitions ---
const unitSets = {
  length: ['mm', 'cm', 'm', 'km', 'in', 'ft', 'yd', 'mi'],
  mass: ['mg', 'g', 'kg', 'oz', 'lb'],
  temperature: ['C', 'F', 'K'],
  area: ['mm²', 'cm²', 'm²', 'km²', 'ha', 'in²', 'ft²', 'ac'],
  volume: ['mL', 'L', 'm³', 'fl_oz', 'gal', 'cup'],
  speed: ['m/s', 'km/h', 'mph', 'knot'],
  data: ['B', 'KB', 'MB', 'GB', 'TB', 'KiB', 'MiB', 'GiB']
}

const angleUnits = ['deg', 'rad', 'grad']

function getUnits(catId) {
  if (catId === 'angle') return angleUnits
  return unitSets[catId] || []
}

function makeConverterState(units) {
  return reactive({
    value: null,
    from: units[0],
    to: units[1],
    result: null
  })
}

const converters = reactive({
  length: makeConverterState(unitSets.length),
  mass: makeConverterState(unitSets.mass),
  temperature: makeConverterState(unitSets.temperature),
  area: makeConverterState(unitSets.area),
  volume: makeConverterState(unitSets.volume),
  speed: makeConverterState(unitSets.speed),
  data: makeConverterState(unitSets.data),
  angle: makeConverterState(angleUnits)
})

// --- Cipher state ---
const ciphers = reactive({
  aes: { mode: 'encrypt', password: '', text: '', result: null, error: null },
  base64: { mode: 'encode', text: '', result: null, error: null },
  rot13: { text: '', result: null },
  hash: { algo: 'sha256', text: '', result: null }
})

// --- Conversion tables ---
const LENGTH_TO_M = {
  mm: 0.001, cm: 0.01, m: 1, km: 1000,
  in: 0.0254, ft: 0.3048, yd: 0.9144, mi: 1609.344
}

const MASS_TO_KG = {
  mg: 0.000001, g: 0.001, kg: 1, oz: 0.0283495, lb: 0.453592
}

const AREA_TO_M2 = {
  'mm²': 0.000001, 'cm²': 0.0001, 'm²': 1, 'km²': 1000000,
  ha: 10000, 'in²': 0.00064516, 'ft²': 0.092903, ac: 4046.86
}

const VOLUME_TO_L = {
  mL: 0.001, L: 1, 'm³': 1000, fl_oz: 0.0295735, gal: 3.78541, cup: 0.236588
}

const SPEED_TO_MS = {
  'm/s': 1, 'km/h': 0.277778, mph: 0.44704, knot: 0.514444
}

const DATA_TO_B = {
  B: 1, KB: 1000, MB: 1000000, GB: 1000000000, TB: 1000000000000,
  KiB: 1024, MiB: 1048576, GiB: 1073741824
}

const ANGLE_TO_DEG = {
  deg: 1, rad: 180 / Math.PI, grad: 0.9
}

const tables = {
  length: LENGTH_TO_M,
  mass: MASS_TO_KG,
  area: AREA_TO_M2,
  volume: VOLUME_TO_L,
  speed: SPEED_TO_MS,
  data: DATA_TO_B,
  angle: ANGLE_TO_DEG
}

function convert(value, table, fromUnit, toUnit) {
  if (value == null || isNaN(value)) return null
  const base = value * table[fromUnit]
  return base / table[toUnit]
}

function convertTemperature(value, fromUnit, toUnit) {
  if (value == null || isNaN(value)) return null
  let celsius = value
  if (fromUnit === 'F') celsius = (value - 32) * 5 / 9
  else if (fromUnit === 'K') celsius = value - 273.15

  if (toUnit === 'C') return celsius
  if (toUnit === 'F') return celsius * 9 / 5 + 32
  if (toUnit === 'K') return celsius + 273.15
  return null
}

function convertCurrent(catId) {
  const state = converters[catId]
  if (state.value == null || isNaN(state.value) || state.value === '') {
    state.result = null
    return
  }

  if (catId === 'temperature') {
    state.result = convertTemperature(Number(state.value), state.from, state.to)
    return
  }

  state.result = convert(Number(state.value), tables[catId], state.from, state.to)
}

function formatNumber(num) {
  if (num == null) return ''
  if (Number.isInteger(num)) return num.toString()
  const str = num.toPrecision(10)
  return str.replace(/(\.[0-9]*[1-9])0+$/, '$1').replace(/\.0+$/, '')
}

// --- AES ---
async function aesEncrypt(plaintext, password) {
  const encoder = new TextEncoder()
  const salt = crypto.getRandomValues(new Uint8Array(16))
  const keyMaterial = await crypto.subtle.importKey(
    'raw', encoder.encode(password), 'PBKDF2', false, ['deriveKey']
  )
  const key = await crypto.subtle.deriveKey(
    { name: 'PBKDF2', salt, iterations: 100000, hash: 'SHA-256' },
    keyMaterial,
    { name: 'AES-CBC', length: 256 },
    false,
    ['encrypt']
  )
  const iv = crypto.getRandomValues(new Uint8Array(16))
  const encrypted = await crypto.subtle.encrypt(
    { name: 'AES-CBC', iv },
    key,
    encoder.encode(plaintext)
  )
  const saltHex = [...salt].map(b => b.toString(16).padStart(2, '0')).join('')
  const ivHex = [...iv].map(b => b.toString(16).padStart(2, '0')).join('')
  const ctHex = [...new Uint8Array(encrypted)].map(b => b.toString(16).padStart(2, '0')).join('')
  return saltHex + ivHex + ctHex
}

async function aesDecrypt(ciphertextHex, password) {
  const encoder = new TextEncoder()
  const bytes = new Uint8Array(ciphertextHex.match(/.{1,2}/g).map(b => parseInt(b, 16)))
  const salt = bytes.slice(0, 16)
  const iv = bytes.slice(16, 32)
  const ct = bytes.slice(32)

  const keyMaterial = await crypto.subtle.importKey(
    'raw', encoder.encode(password), 'PBKDF2', false, ['deriveKey']
  )
  const key = await crypto.subtle.deriveKey(
    { name: 'PBKDF2', salt, iterations: 100000, hash: 'SHA-256' },
    keyMaterial,
    { name: 'AES-CBC', length: 256 },
    false,
    ['decrypt']
  )
  const decrypted = await crypto.subtle.decrypt({ name: 'AES-CBC', iv }, key, ct)
  return new TextDecoder().decode(decrypted)
}

async function handleAes() {
  ciphers.aes.error = null
  if (!ciphers.aes.text || !ciphers.aes.password) {
    ciphers.aes.result = null
    return
  }

  cipherWorking.value = true
  try {
    if (ciphers.aes.mode === 'encrypt') {
      ciphers.aes.result = await aesEncrypt(ciphers.aes.text, ciphers.aes.password)
    } else {
      ciphers.aes.result = await aesDecrypt(ciphers.aes.text, ciphers.aes.password)
    }
  } catch (e) {
    ciphers.aes.error = e.message || 'Decryption failed. Check your password and ciphertext.'
    ciphers.aes.result = null
  } finally {
    cipherWorking.value = false
  }
}

// --- Base64 ---
function handleBase64() {
  ciphers.base64.error = null
  if (!ciphers.base64.text) {
    ciphers.base64.result = null
    return
  }

  try {
    if (ciphers.base64.mode === 'encode') {
      ciphers.base64.result = btoa(unescape(encodeURIComponent(ciphers.base64.text)))
    } else {
      ciphers.base64.result = decodeURIComponent(escape(atob(ciphers.base64.text)))
    }
  } catch (e) {
    ciphers.base64.error = e.message || 'Invalid input for this operation.'
    ciphers.base64.result = null
  }
}

// --- ROT13 ---
function rot13(str) {
  return str.replace(/[a-zA-Z]/g, (ch) => {
    const base = ch <= 'Z' ? 65 : 97
    return String.fromCharCode(((ch.charCodeAt(0) - base + 13) % 26) + base)
  })
}

function handleRot13() {
  if (!ciphers.rot13.text) {
    ciphers.rot13.result = null
    return
  }
  ciphers.rot13.result = rot13(ciphers.rot13.text)
}

// --- Hash / Checksum ---
async function sha1(text) {
  const encoder = new TextEncoder()
  const hash = await crypto.subtle.digest('SHA-1', encoder.encode(text))
  return [...new Uint8Array(hash)].map(b => b.toString(16).padStart(2, '0')).join('')
}

async function sha256(text) {
  const encoder = new TextEncoder()
  const hash = await crypto.subtle.digest('SHA-256', encoder.encode(text))
  return [...new Uint8Array(hash)].map(b => b.toString(16).padStart(2, '0')).join('')
}

async function sha512(text) {
  const encoder = new TextEncoder()
  const hash = await crypto.subtle.digest('SHA-512', encoder.encode(text))
  return [...new Uint8Array(hash)].map(b => b.toString(16).padStart(2, '0')).join('')
}

function md5(text) {
  function r(value, count) { return (value << count) | (value >>> (32 - count)) }

  const str = unescape(encodeURIComponent(text))
  const len = str.length
  const words = new Array(len + 8)
  let i = 0
  for (; i < len; i++) { words[i] = str.charCodeAt(i) }
  words[i++] = 0x80
  while (i % 64 !== 56) { words[i++] = 0 }
  const bitLenLo = len * 8
  words[i++] = bitLenLo & 0xff
  words[i++] = (bitLenLo >>> 8) & 0xff
  words[i++] = (bitLenLo >>> 16) & 0xff
  words[i++] = (bitLenLo >>> 24) & 0xff

  let a = 0x67452301, b = 0xefcdab89, c = 0x98badcfe, d = 0x10325476

  for (let j = 0; j < words.length; j += 64) {
    const chunk = words.slice(j, j + 64)
    const x = new Array(16)
    for (let k = 0; k < 16; k++) {
      x[k] = (chunk[k * 4] | (chunk[k * 4 + 1] << 8) | (chunk[k * 4 + 2] << 16) | (chunk[k * 4 + 3] << 24)) >>> 0
    }
    let aa = a, bb = b, cc = c, dd = d
    for (let k = 0; k < 64; k++) {
      let f, g
      if (k < 16) { f = (bb & cc) | (~bb & dd); g = k }
      else if (k < 32) { f = (dd & bb) | (~dd & cc); g = (5 * k + 1) % 16 }
      else if (k < 48) { f = bb ^ cc ^ dd; g = (3 * k + 5) % 16 }
      else { f = cc ^ (bb | ~dd); g = (7 * k) % 16 }
      const temp = dd
      dd = cc
      cc = bb
      const tVal = ([0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
        0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
        0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
        0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
        0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391][k])
      bb = (bb + r((aa + f + (x[g] >>> 0) + (tVal >>> 0)) >>> 0, [7,12,17,22,5,9,14,20,4,11,16,23,6,10,15,21][k % 16])) >>> 0
      aa = temp
    }
    a = (a + aa) >>> 0; b = (b + bb) >>> 0; c = (c + cc) >>> 0; d = (d + dd) >>> 0
  }

  return [a, b, c, d].map(v => ('0000000' + v.toString(16)).slice(-8)).join('')
}

async function handleHash() {
  if (!ciphers.hash.text) {
    ciphers.hash.result = null
    return
  }

  const algo = ciphers.hash.algo
  if (algo === 'sha256') {
    ciphers.hash.result = await sha256(ciphers.hash.text)
  } else if (algo === 'sha512') {
    ciphers.hash.result = await sha512(ciphers.hash.text)
  } else if (algo === 'sha1') {
    ciphers.hash.result = await sha1(ciphers.hash.text)
  } else {
    ciphers.hash.result = md5(ciphers.hash.text)
  }
}
</script>

<style scoped>
.tools-page {
  height: 100%;
}

.tools-layout {
  display: grid;
  grid-template-columns: 280px 1fr;
  gap: 1rem;
  min-height: calc(100vh - 230px);
}

.tools-sidebar,
.tools-panel {
  background: #2a2a2a;
  border: 1px solid #3a3a3a;
  border-radius: 10px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
}

.tools-sidebar {
  display: flex;
  flex-direction: column;
  transition: width 0.2s ease;
  position: sticky;
  top: 1rem;
  align-self: start;
}

.tools-sidebar.collapsed {
  width: 64px;
}

.sidebar-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 1rem;
  border-bottom: 1px solid #3a3a3a;
}

.sidebar-header h3 {
  margin: 0;
  color: #e0e0e0;
  font-size: 1rem;
}

.icon-btn {
  background: #1a1a1a;
  color: #d9d9d9;
  border: 1px solid #4a4a4a;
  border-radius: 6px;
  width: 32px;
  height: 32px;
  cursor: pointer;
  font-size: 1rem;
}

.sidebar-content {
  padding: 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.sidebar-section {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.sidebar-section-title {
  margin: 0 0 0.25rem;
  color: #8d8d8d;
  font-size: 0.72rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  padding: 0 0.5rem;
}

.sidebar-item {
  display: block;
  width: 100%;
  text-align: left;
  padding: 0.55rem 0.65rem;
  border-radius: 6px;
  border: 1px solid transparent;
  background: transparent;
  color: #c0c0c0;
  cursor: pointer;
  font-size: 0.88rem;
  text-decoration: none;
  transition: background 0.15s, border-color 0.15s, color 0.15s;
}

.sidebar-item:hover {
  background: #1a1a1a;
  color: #e0e0e0;
}

.sidebar-item.active {
  background: #253025;
  border-color: #77b255;
  color: #8fd28f;
  font-weight: 600;
}

.tools-panel {
  padding: 1.25rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
  overflow-y: auto;
}

.panel-header {
  border-bottom: 1px solid #3a3a3a;
  padding-bottom: 0.75rem;
}

.panel-header h2 {
  margin: 0;
  color: #e0e0e0;
  font-size: 1.15rem;
}

.converter-card,
.cipher-card {
  background: #1a1a1a;
  border: 1px solid #3a3a3a;
  border-radius: 10px;
  overflow: hidden;
  border-left: 4px solid #667eea;
}

.cipher-card {
  border-left-color: #7b5cff;
}

.card-header {
  padding: 0.75rem 1.25rem;
  border-bottom: 1px solid #3a3a3a;
  background: rgba(0, 0, 0, 0.2);
}

.card-header h3 {
  margin: 0;
  color: #e0e0e0;
  font-size: 0.95rem;
  font-weight: 600;
}

.converter-body,
.cipher-body {
  padding: 1.25rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.converter-input-row,
.cipher-input-row {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
  align-items: center;
}

.converter-result-row,
.cipher-result-row {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.tool-label {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  color: #b0b0b0;
  font-size: 0.82rem;
  flex: 1;
  min-width: 180px;
}

.tool-label-wide {
  flex: 2;
  min-width: 240px;
}

.tool-input {
  width: 160px;
  background: #2a2a2a;
  color: #e0e0e0;
  border: 1px solid #3a3a3a;
  border-radius: 6px;
  padding: 0.6rem 0.75rem;
  font-size: 0.95rem;
}

.tool-input:focus {
  outline: 2px solid rgba(102, 126, 234, 0.5);
  outline-offset: 1px;
}

.tool-select {
  background: #2a2a2a;
  color: #e0e0e0;
  border: 1px solid #3a3a3a;
  border-radius: 6px;
  padding: 0.6rem 0.75rem;
  font-size: 0.9rem;
  min-width: 90px;
}

.tool-select:focus {
  outline: 2px solid rgba(102, 126, 234, 0.5);
  outline-offset: 1px;
}

.arrow {
  color: #667eea;
  font-weight: bold;
  font-size: 1.2rem;
}

.tool-textarea {
  width: 100%;
  min-width: 240px;
  background: #2a2a2a;
  color: #e0e0e0;
  border: 1px solid #3a3a3a;
  border-radius: 6px;
  padding: 0.6rem 0.75rem;
  font-size: 0.88rem;
  resize: vertical;
  font-family: inherit;
}

.tool-textarea:focus {
  outline: 2px solid rgba(102, 126, 234, 0.5);
  outline-offset: 1px;
}

.cipher-actions {
  display: flex;
  gap: 0.5rem;
  align-items: center;
}

.btn {
  border: 0;
  border-radius: 8px;
  padding: 0.6rem 1rem;
  font-weight: 600;
  cursor: pointer;
  font-size: 0.88rem;
}

.btn-primary {
  background: #6291ff;
  color: #fff;
}

.btn-primary:hover {
  background: #779fff;
}

.btn-primary:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.tool-result {
  width: 100%;
  background: #2a2a2a;
  border: 1px solid #3a3a3a;
  border-radius: 6px;
  padding: 0.6rem 0.75rem;
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
  align-items: flex-start;
}

.result-label {
  color: #8d8d8d;
  font-size: 0.78rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.result-value {
  color: #667eea;
  font-weight: 700;
  font-size: 1.05rem;
  word-break: break-all;
}

.result-code {
  color: #90ee90;
  font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
  font-size: 0.84rem;
  word-break: break-all;
  line-height: 1.5;
  white-space: pre-wrap;
}

.cipher-error {
  width: 100%;
  color: #ff8a8a;
  font-size: 0.85rem;
  margin: 0;
}

@media (max-width: 1024px) {
  .tools-layout {
    grid-template-columns: 1fr;
  }

  .tools-sidebar {
    position: static;
    max-height: none;
  }

  .tools-sidebar.collapsed {
    width: 100%;
  }
}

@media (max-width: 768px) {
  .converter-input-row,
  .cipher-input-row {
    flex-direction: column;
    align-items: stretch;
  }

  .tool-input {
    width: 100%;
  }
}
</style>