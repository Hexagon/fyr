<template>
  <div class="tools-page">
    <div class="tools-layout">
      <aside class="tools-sidebar" :class="{ collapsed: sidebarCollapsed }">
        <div class="sidebar-header">
          <h3>Tools</h3>
          <button
            class="icon-btn"
            :aria-label="sidebarCollapsed ? 'Expand tools navigation panel' : 'Collapse tools navigation panel'"
            :title="sidebarCollapsed ? 'Expand panel' : 'Collapse panel'"
            :aria-expanded="String(!sidebarCollapsed)"
            aria-controls="tools-sidebar-content"
            @click="toggleSidebar"
          >
            {{ sidebarCollapsed ? '»' : '«' }}
          </button>
        </div>

        <div v-if="!sidebarCollapsed" id="tools-sidebar-content" class="sidebar-content">
          <div class="sidebar-section">
            <h4 class="sidebar-section-title">Length & Speed</h4>
            <a
              v-for="cat in converterGroups.length_speed"
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
            <h4 class="sidebar-section-title">Weight & Volume</h4>
            <a
              v-for="cat in converterGroups.weight_volume"
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
            <h4 class="sidebar-section-title">Area & Angle</h4>
            <a
              v-for="cat in converterGroups.area_angle"
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
            <h4 class="sidebar-section-title">Temperature</h4>
            <a
              v-for="cat in converterGroups.temperature"
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
            <h4 class="sidebar-section-title">Digital Storage</h4>
            <a
              v-for="cat in converterGroups.digital"
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
            <h4 class="sidebar-section-title">Energy & Power</h4>
            <a
              v-for="cat in converterGroups.energy_power"
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
            <h4 class="sidebar-section-title">Pressure & Time</h4>
            <a
              v-for="cat in converterGroups.pressure_time"
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
              :class="{ active: activeTab === 'ciphers' && activeCipher === tool.id }"
              @click="selectCipher(tool.id)"
            >
              {{ tool.label }}
            </button>
          </div>
        </div>
      </aside>

      <section class="tools-panel">
        <div class="tools-topbar">
          <div class="tools-tabs" role="group" aria-label="Tool category tabs">
            <button
              type="button"
              class="tools-tab"
              :class="{ active: activeTab === 'converters' }"
              :aria-pressed="String(activeTab === 'converters')"
              @click="showConverters"
            >
              Converters
            </button>
            <button
              type="button"
              class="tools-tab"
              :class="{ active: activeTab === 'ciphers' }"
              :aria-pressed="String(activeTab === 'ciphers')"
              @click="showCiphers"
            >
              Encryption & Ciphers
            </button>
          </div>
        </div>

        <!-- Unit Converters Panel -->
        <template v-if="activeTab === 'converters'">
          <div class="panel-header">
            <h2>Unit Converters</h2>
          </div>

          <div class="converter-group">
            <h3 class="group-heading">Length & Speed</h3>
            <div
              v-for="cat in converterGroups.length_speed"
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
          </div>

          <div class="converter-group">
            <h3 class="group-heading">Weight & Volume</h3>
            <div
              v-for="cat in converterGroups.weight_volume"
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
          </div>

          <div class="converter-group">
            <h3 class="group-heading">Area & Angle</h3>
            <div
              v-for="cat in converterGroups.area_angle"
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
          </div>

          <div class="converter-group">
            <h3 class="group-heading">Temperature</h3>
            <div
              v-for="cat in converterGroups.temperature"
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
          </div>

          <div class="converter-group">
            <h3 class="group-heading">Digital Storage</h3>
            <div
              v-for="cat in converterGroups.digital"
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
          </div>

          <div class="converter-group">
            <h3 class="group-heading">Energy & Power</h3>
            <div
              v-for="cat in converterGroups.energy_power"
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
          </div>

          <div class="converter-group">
            <h3 class="group-heading">Pressure & Time</h3>
            <div
              v-for="cat in converterGroups.pressure_time"
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
              <h3>AES-{{ ciphers.aes.bits }}-{{ ciphers.aes.cipher_mode.toUpperCase() }}</h3>
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
                  Key Source
                  <select v-model="ciphers.aes.key_type" class="tool-select">
                    <option value="password">Password (PBKDF2)</option>
                    <option value="raw">Raw Key (hex)</option>
                  </select>
                </label>
                <label class="tool-label">
                  Bits
                  <select v-model="ciphers.aes.bits" class="tool-select">
                    <option value="128">128</option>
                    <option value="192">192</option>
                    <option value="256">256</option>
                  </select>
                </label>
                <label class="tool-label">
                  Cipher Mode
                  <select v-model="ciphers.aes.cipher_mode" class="tool-select">
                    <option value="gcm">GCM</option>
                    <option value="cbc">CBC</option>
                    <option value="ecb">ECB</option>
                  </select>
                </label>
              </div>

              <div class="cipher-info" v-if="ciphers.aes.key_type === 'password'">
                Key derivation: <strong>PBKDF2-HMAC-SHA256, 100,000 iterations</strong>.
                Salt is prepended to the output.
              </div>
              <div class="cipher-info cipher-warning" v-if="ciphers.aes.cipher_mode === 'ecb'">
                ⚠ ECB is not authenticated and reveals plaintext patterns. Use CBC or GCM for sensitive data.
              </div>
              <div class="cipher-info cipher-warning" v-if="ciphers.aes.cipher_mode === 'gcm' && ciphers.aes.bits !== '256'">
                ⚠ GCM mode only supports 256-bit keys.
              </div>

              <div class="cipher-input-row">
                <label class="tool-label">
                  {{ ciphers.aes.key_type === 'password' ? 'Password' : 'Key (hex)' }}
                  <input
                    v-model="ciphers.aes.key_source"
                    type="text"
                    class="tool-input"
                    :placeholder="ciphers.aes.key_type === 'password' ? 'Enter password' : 'Enter hex key'"
                  />
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
import { apiService } from '../services/api.js'

const sidebarCollapsed = ref(false)
const activeTab = ref('converters')
const activeConverter = ref('length')
const activeCipher = ref('aes')
const cipherWorking = ref(false)

const converterGroups = {
  length_speed: [
    { id: 'length', label: 'Length' },
    { id: 'speed', label: 'Speed' }
  ],
  weight_volume: [
    { id: 'mass', label: 'Mass' },
    { id: 'volume', label: 'Volume' }
  ],
  area_angle: [
    { id: 'area', label: 'Area' },
    { id: 'angle', label: 'Angle' }
  ],
  temperature: [
    { id: 'temperature', label: 'Temperature' }
  ],
  digital: [
    { id: 'data', label: 'Data' }
  ],
  energy_power: [
    { id: 'energy', label: 'Energy' },
    { id: 'power', label: 'Power' }
  ],
  pressure_time: [
    { id: 'pressure', label: 'Pressure' },
    { id: 'time', label: 'Time' }
  ]
}

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

const showConverters = () => {
  activeTab.value = 'converters'
}

const showCiphers = () => {
  activeTab.value = 'ciphers'
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
  data: ['B', 'KB', 'MB', 'GB', 'TB', 'KiB', 'MiB', 'GiB'],
  angle: ['deg', 'rad', 'grad'],
  pressure: ['Pa', 'kPa', 'MPa', 'bar', 'psi', 'atm', 'mmHg'],
  energy: ['J', 'kJ', 'cal', 'kcal', 'Wh', 'kWh'],
  power: ['W', 'kW', 'MW', 'HP', 'BTU/h'],
  time: ['ms', 's', 'min', 'h', 'day']
}

function getUnits(catId) {
  return unitSets[catId] || []
}

function makeConverterState(units) {
  return reactive({
    value: null,
    from: units[0],
    to: units.length > 1 ? units[1] : units[0],
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
  angle: makeConverterState(unitSets.angle),
  pressure: makeConverterState(unitSets.pressure),
  energy: makeConverterState(unitSets.energy),
  power: makeConverterState(unitSets.power),
  time: makeConverterState(unitSets.time)
})

// --- Cipher state ---
const ciphers = reactive({
  aes: { mode: 'encrypt', key_type: 'password', key_source: '', bits: '256', cipher_mode: 'gcm', text: '', result: null, error: null },
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

const PRESSURE_TO_PA = {
  Pa: 1, kPa: 1000, MPa: 1000000, bar: 100000,
  psi: 6894.76, atm: 101325, mmHg: 133.322
}

const ENERGY_TO_J = {
  J: 1, kJ: 1000, cal: 4.184, kcal: 4184, Wh: 3600, kWh: 3600000
}

const POWER_TO_W = {
  W: 1, kW: 1000, MW: 1000000, HP: 745.7, 'BTU/h': 0.293071
}

const TIME_TO_S = {
  ms: 0.001, s: 1, min: 60, h: 3600, day: 86400
}

const tables = {
  length: LENGTH_TO_M,
  mass: MASS_TO_KG,
  area: AREA_TO_M2,
  volume: VOLUME_TO_L,
  speed: SPEED_TO_MS,
  data: DATA_TO_B,
  angle: ANGLE_TO_DEG,
  pressure: PRESSURE_TO_PA,
  energy: ENERGY_TO_J,
  power: POWER_TO_W,
  time: TIME_TO_S
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
async function handleAes() {
  ciphers.aes.error = null
  if (!ciphers.aes.text || !ciphers.aes.key_source) {
    ciphers.aes.result = null
    return
  }

  cipherWorking.value = true
  try {
    const result = await apiService.toolsAes({
      mode: ciphers.aes.mode,
      key_type: ciphers.aes.key_type,
      key_source: ciphers.aes.key_source,
      bits: parseInt(ciphers.aes.bits),
      cipher_mode: ciphers.aes.cipher_mode,
      text: ciphers.aes.text
    })
    ciphers.aes.result = result.result
  } catch (e) {
    const msg = e?.response?.data?.message || e.message || ''
    ciphers.aes.error = msg || 'Operation failed. Check your password and input.'
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
async function handleHash() {
  if (!ciphers.hash.text) {
    ciphers.hash.result = null
    return
  }

  try {
    const result = await apiService.toolsHash({
      algo: ciphers.hash.algo,
      text: ciphers.hash.text
    })
    ciphers.hash.result = result.result
  } catch (e) {
    ciphers.hash.result = null
  }
}
</script>

<style scoped>
.tools-page {
  --panel: #1f2428;
  --panel-soft: #252d33;
  --panel-ink: #12161a;
  --line: #39434c;
  --text: #e7edf3;
  --muted: #a8b2bc;

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
  background: linear-gradient(180deg, var(--panel) 0%, var(--panel-ink) 100%);
  border: 1px solid var(--line);
  border-radius: 10px;
  box-shadow: 0 12px 36px rgba(0, 0, 0, 0.18);
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
  border-bottom: 1px solid var(--line);
}

.sidebar-header h3 {
  margin: 0;
  color: var(--text);
  font-size: 1rem;
}

.icon-btn {
  background: #141a1f;
  color: var(--text);
  border: 1px solid #4f5d68;
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
  color: var(--muted);
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
  color: #d0d8e0;
  cursor: pointer;
  font-size: 0.88rem;
  text-decoration: none;
  transition: background 0.15s, border-color 0.15s, color 0.15s;
}

.sidebar-item:hover {
  background: #11161a;
  color: var(--text);
}

.sidebar-item.active {
  background: #1a3f44;
  border-color: #40c0b5;
  color: #b9fff6;
  font-weight: 600;
}

.tools-panel {
  padding: 1.25rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
  overflow-y: auto;
}

.tools-topbar {
  display: flex;
  align-items: center;
  justify-content: flex-start;
}

.tools-tabs {
  display: inline-flex;
  gap: 0.35rem;
  border: 1px solid var(--line);
  border-radius: 999px;
  padding: 0.2rem;
  background: rgba(17, 22, 26, 0.75);
}

.tools-tab {
  border: 1px solid transparent;
  border-radius: 999px;
  background: transparent;
  color: var(--muted);
  font-size: 0.82rem;
  font-weight: 600;
  padding: 0.35rem 0.75rem;
  cursor: pointer;
}

.tools-tab:hover {
  color: var(--text);
}

.tools-tab.active {
  border-color: rgba(64, 192, 181, 0.5);
  background: rgba(64, 192, 181, 0.2);
  color: #b9fff6;
}

.panel-header {
  border-bottom: 1px solid var(--line);
  padding-bottom: 0.75rem;
}

.panel-header h2 {
  margin: 0;
  color: var(--text);
  font-size: 1.15rem;
}

.converter-group {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.group-heading {
  margin: 0.5rem 0 0 0;
  color: var(--muted);
  font-size: 0.78rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  padding: 0 0.25rem;
}

.converter-card,
.cipher-card {
  background: #141a1f;
  border: 1px solid var(--line);
  border-radius: 10px;
  overflow: hidden;
  border-left: 4px solid #667eea;
}

.cipher-card {
  border-left-color: #7b5cff;
}

.card-header {
  padding: 0.75rem 1.25rem;
  border-bottom: 1px solid var(--line);
  background: rgba(0, 0, 0, 0.2);
}

.card-header h3 {
  margin: 0;
  color: var(--text);
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
  color: var(--muted);
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
  background: #11161a;
  color: var(--text);
  border: 1px solid var(--line);
  border-radius: 6px;
  padding: 0.6rem 0.75rem;
  font-size: 0.95rem;
}

.tool-input:focus {
  outline: 2px solid rgba(102, 126, 234, 0.5);
  outline-offset: 1px;
}

.tool-select {
  background: #11161a;
  color: var(--text);
  border: 1px solid var(--line);
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
  background: #11161a;
  color: var(--text);
  border: 1px solid var(--line);
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
  background: #11161a;
  border: 1px solid var(--line);
  border-radius: 6px;
  padding: 0.6rem 0.75rem;
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
  align-items: flex-start;
}

.result-label {
  color: var(--muted);
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

.cipher-info {
  padding: 0.5rem 0.75rem;
  background: #11161a;
  border: 1px solid var(--line);
  border-radius: 6px;
  color: var(--muted);
  font-size: 0.82rem;
}

.cipher-warning {
  border-color: #8a6a00;
  background: #2a2200;
  color: #d4a843;
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
  .tools-tab {
    font-size: 0.75rem;
    padding: 0.32rem 0.6rem;
  }

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