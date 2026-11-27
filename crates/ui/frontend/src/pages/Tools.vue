<template>
  <div class="tools-page">
    <div class="tools-layout">
      <aside class="tools-sidebar" :class="{ collapsed: sidebarCollapsed }">
        <div class="sidebar-header">
          <h3 v-if="!sidebarCollapsed">{{ t('nav.tools') }}</h3>
          <button
            class="icon-btn"
            :aria-label="sidebarCollapsed ? t('tools.expandNav') : t('tools.collapseNav')"
            :title="sidebarCollapsed ? t('common.expandPanel') : t('common.collapsePanel')"
            :aria-expanded="String(!sidebarCollapsed)"
            aria-controls="tools-sidebar-content"
            @click="toggleSidebar"
          >
            {{ sidebarCollapsed ? '»' : '«' }}
          </button>
        </div>

        <div v-if="!sidebarCollapsed" id="tools-sidebar-content" class="sidebar-content">
          <!-- Converters sidebar -->
          <template v-if="activeTab === 'converters'">
            <div class="sidebar-section">
              <h4 class="sidebar-section-title">{{ t('tools.groups.length_speed') }}</h4>
              <a v-for="cat in converterGroups.length_speed" :key="cat.id" :href="'#' + cat.id" class="sidebar-item"
                :class="{ active: activeConverter === cat.id }" @click.prevent="scrollToConverter(cat.id)">
                {{ t(cat.labelKey) }}
              </a>
            </div>
            <div class="sidebar-section">
              <h4 class="sidebar-section-title">{{ t('tools.groups.weight_volume') }}</h4>
              <a v-for="cat in converterGroups.weight_volume" :key="cat.id" :href="'#' + cat.id" class="sidebar-item"
                :class="{ active: activeConverter === cat.id }" @click.prevent="scrollToConverter(cat.id)">
                {{ t(cat.labelKey) }}
              </a>
            </div>
            <div class="sidebar-section">
              <h4 class="sidebar-section-title">{{ t('tools.groups.area_angle') }}</h4>
              <a v-for="cat in converterGroups.area_angle" :key="cat.id" :href="'#' + cat.id" class="sidebar-item"
                :class="{ active: activeConverter === cat.id }" @click.prevent="scrollToConverter(cat.id)">
                {{ t(cat.labelKey) }}
              </a>
            </div>
            <div class="sidebar-section">
              <h4 class="sidebar-section-title">{{ t('tools.groups.temperature') }}</h4>
              <a v-for="cat in converterGroups.temperature" :key="cat.id" :href="'#' + cat.id" class="sidebar-item"
                :class="{ active: activeConverter === cat.id }" @click.prevent="scrollToConverter(cat.id)">
                {{ t(cat.labelKey) }}
              </a>
            </div>
            <div class="sidebar-section">
              <h4 class="sidebar-section-title">{{ t('tools.groups.digital') }}</h4>
              <a v-for="cat in converterGroups.digital" :key="cat.id" :href="'#' + cat.id" class="sidebar-item"
                :class="{ active: activeConverter === cat.id }" @click.prevent="scrollToConverter(cat.id)">
                {{ t(cat.labelKey) }}
              </a>
            </div>
            <div class="sidebar-section">
              <h4 class="sidebar-section-title">{{ t('tools.groups.energy_power') }}</h4>
              <a v-for="cat in converterGroups.energy_power" :key="cat.id" :href="'#' + cat.id" class="sidebar-item"
                :class="{ active: activeConverter === cat.id }" @click.prevent="scrollToConverter(cat.id)">
                {{ t(cat.labelKey) }}
              </a>
            </div>
            <div class="sidebar-section">
              <h4 class="sidebar-section-title">{{ t('tools.groups.pressure_time') }}</h4>
              <a v-for="cat in converterGroups.pressure_time" :key="cat.id" :href="'#' + cat.id" class="sidebar-item"
                :class="{ active: activeConverter === cat.id }" @click.prevent="scrollToConverter(cat.id)">
                {{ t(cat.labelKey) }}
              </a>
            </div>
            <div class="sidebar-section">
              <h4 class="sidebar-section-title">{{ t('tools.groups.nav_weather') }}</h4>
              <a v-for="cat in converterGroups.nav_weather" :key="cat.id" :href="'#' + cat.id" class="sidebar-item"
                :class="{ active: activeConverter === cat.id }" @click.prevent="scrollToConverter(cat.id)">
                {{ t(cat.labelKey) }}
              </a>
            </div>
          </template>

          <!-- Ciphers sidebar -->
          <template v-if="activeTab === 'ciphers'">
            <div class="sidebar-section">
              <h4 class="sidebar-section-title">{{ t('tools.tabs.ciphers') }}</h4>
              <button v-for="tool in cipherTools" :key="tool.id" class="sidebar-item"
                :class="{ active: activeCipher === tool.id }" @click="selectCipher(tool.id)">
                {{ t(tool.labelKey) }}
              </button>
            </div>
          </template>

          <!-- Text & Numbers sidebar -->
          <template v-if="activeTab === 'text'">
            <div class="sidebar-section">
              <h4 class="sidebar-section-title">{{ t('tools.textTools') }}</h4>
              <button v-for="tool in textTools" :key="tool.id" class="sidebar-item"
                :class="{ active: activeTextTool === tool.id }" @click="activeTextTool = tool.id">
                {{ t(tool.labelKey) }}
              </button>
            </div>
            <div class="sidebar-section">
              <h4 class="sidebar-section-title">{{ t('tools.numberTools') }}</h4>
              <button v-for="tool in numberTools" :key="tool.id" class="sidebar-item"
                :class="{ active: activeTextTool === tool.id }" @click="activeTextTool = tool.id">
                {{ t(tool.labelKey) }}
              </button>
            </div>
          </template>

          <!-- Date & Time sidebar -->
          <template v-if="activeTab === 'datetime'">
            <div class="sidebar-section">
              <h4 class="sidebar-section-title">{{ t('tools.tabs.datetime') }}</h4>
              <button v-for="tool in datetimeTools" :key="tool.id" class="sidebar-item"
                :class="{ active: activeDatetimeTool === tool.id }" @click="activeDatetimeTool = tool.id">
                {{ t(tool.labelKey) }}
              </button>
            </div>
          </template>
        </div>
      </aside>

      <section class="tools-panel">
        <div class="tools-topbar">
          <div class="tools-tabs" role="group" :aria-label="t('tools.tabsLabel')">
            <button type="button" class="tools-tab" :class="{ active: activeTab === 'converters' }"
              :aria-pressed="String(activeTab === 'converters')" @click="activeTab = 'converters'">
              {{ t('tools.tabs.converters') }}
            </button>
            <button type="button" class="tools-tab" :class="{ active: activeTab === 'text' }"
              :aria-pressed="String(activeTab === 'text')" @click="activeTab = 'text'">
              {{ t('tools.tabs.text') }}
            </button>
            <button type="button" class="tools-tab" :class="{ active: activeTab === 'datetime' }"
              :aria-pressed="String(activeTab === 'datetime')" @click="activeTab = 'datetime'">
              {{ t('tools.tabs.datetime') }}
            </button>
            <button type="button" class="tools-tab" :class="{ active: activeTab === 'ciphers' }"
              :aria-pressed="String(activeTab === 'ciphers')" @click="activeTab = 'ciphers'">
              {{ t('tools.tabs.ciphers') }}
            </button>
          </div>
        </div>

        <!-- Unit Converters Panel -->
        <template v-if="activeTab === 'converters'">
          <div class="panel-header">
            <h2>{{ t('tools.unitConverters') }}</h2>
          </div>

          <div v-for="group in converterCardGroups" :key="group.key" class="converter-group">
            <h3 class="group-heading">{{ t(group.labelKey) }}</h3>
            <div v-for="cat in group.categories" :key="cat.id" :id="cat.id" class="converter-card">
              <div class="card-header"><h3>{{ t(cat.labelKey) }}</h3></div>
              <div class="converter-body">
                <div class="converter-input-row">
                  <input v-model.number="converters[cat.id].value" type="number" step="any"
                    :placeholder="t('tools.enterValue')" :aria-label="t('tools.enterValue')" class="tool-input" @input="convertCurrent(cat.id)" />
                  <select v-model="converters[cat.id].from" class="tool-select" :aria-label="t('tools.fromUnit')" @change="convertCurrent(cat.id)">
                    <option v-for="u in getUnits(cat.id)" :key="u" :value="u">{{ unitLabel(u) }}</option>
                  </select>
                  <span class="arrow">→</span>
                  <select v-model="converters[cat.id].to" class="tool-select" :aria-label="t('tools.toUnit')" @change="convertCurrent(cat.id)">
                    <option v-for="u in getUnits(cat.id)" :key="u" :value="u">{{ unitLabel(u) }}</option>
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

          <!-- Coordinate Converter -->
          <div class="converter-group">
            <h3 class="group-heading">{{ t('tools.groups.nav_weather') }}</h3>

            <div id="coordinates" class="converter-card">
              <div class="card-header"><h3>{{ t('tools.converters.coordinates') }}</h3></div>
              <div class="converter-body">
                <div class="cipher-input-row">
                  <label class="tool-label">
                    {{ t('tools.coord.direction') }}
                    <select v-model="coordMode" class="tool-select">
                      <option value="to_dms">{{ t('tools.coord.toDms') }}</option>
                      <option value="to_dec">{{ t('tools.coord.toDec') }}</option>
                    </select>
                  </label>
                </div>
                <div v-if="coordMode === 'to_dms'" class="cipher-input-row">
                  <label class="tool-label">
                    {{ t('tools.coord.latDec') }}
                    <input v-model.number="coordDec.lat" type="number" step="any" min="-90" max="90"
                      class="tool-input" :placeholder="t('tools.coord.range', { min: -90, max: 90 })" @input="convertCoordToDms" />
                  </label>
                  <label class="tool-label">
                    {{ t('tools.coord.lonDec') }}
                    <input v-model.number="coordDec.lon" type="number" step="any" min="-180" max="180"
                      class="tool-input" :placeholder="t('tools.coord.range', { min: -180, max: 180 })" @input="convertCoordToDms" />
                  </label>
                </div>
                <div v-else class="cipher-input-row">
                  <label class="tool-label">{{ t('tools.coord.latD') }}<input v-model.number="coordDms.latD" type="number" step="1" class="tool-input" @input="convertCoordToDecimal" /></label>
                  <label class="tool-label">{{ t('tools.coord.latM') }}<input v-model.number="coordDms.latM" type="number" step="1" min="0" max="59" class="tool-input" @input="convertCoordToDecimal" /></label>
                  <label class="tool-label">{{ t('tools.coord.latS') }}<input v-model.number="coordDms.latS" type="number" step="any" min="0" max="59.999" class="tool-input" @input="convertCoordToDecimal" /></label>
                  <label class="tool-label">N/S<select v-model="coordDms.latDir" class="tool-select" @change="convertCoordToDecimal"><option>N</option><option>S</option></select></label>
                  <label class="tool-label">{{ t('tools.coord.lonD') }}<input v-model.number="coordDms.lonD" type="number" step="1" class="tool-input" @input="convertCoordToDecimal" /></label>
                  <label class="tool-label">{{ t('tools.coord.lonM') }}<input v-model.number="coordDms.lonM" type="number" step="1" min="0" max="59" class="tool-input" @input="convertCoordToDecimal" /></label>
                  <label class="tool-label">{{ t('tools.coord.lonS') }}<input v-model.number="coordDms.lonS" type="number" step="any" min="0" max="59.999" class="tool-input" @input="convertCoordToDecimal" /></label>
                  <label class="tool-label">E/W<select v-model="coordDms.lonDir" class="tool-select" @change="convertCoordToDecimal"><option>E</option><option>W</option></select></label>
                </div>
                <div class="converter-result-row">
                  <div class="tool-result" v-if="coordResult">
                    <span class="result-code">{{ coordResult }}</span>
                  </div>
                </div>
              </div>
            </div>

            <!-- Wind Chill -->
            <div id="windchill" class="converter-card">
              <div class="card-header"><h3>{{ t('tools.converters.windchill') }}</h3></div>
              <div class="converter-body">
                <div class="cipher-info">{{ t('tools.windchill.info') }}</div>
                <div class="cipher-input-row">
                  <label class="tool-label">
                    {{ t('tools.temperature') }}
                    <input v-model.number="windChill.temp" type="number" step="any" class="tool-input" :placeholder="t('tools.example', { value: 0 })" @input="calcWindChill" />
                  </label>
                  <label class="tool-label">
                    {{ t('tools.windchill.wind') }}
                    <input v-model.number="windChill.wind" type="number" step="any" min="0" class="tool-input" :placeholder="t('tools.example', { value: 20 })" @input="calcWindChill" />
                  </label>
                  <label class="tool-label">
                    {{ t('tools.units.label') }}
                    <select v-model="windChill.units" class="tool-select" @change="calcWindChill">
                      <option value="metric">°C / km/h</option>
                      <option value="imperial">°F / mph</option>
                    </select>
                  </label>
                </div>
                <div class="converter-result-row">
                  <div class="tool-result" v-if="windChill.result !== null">
                    <span class="result-label">{{ t('tools.feelsLike') }}</span>
                    <span class="result-value">{{ windChill.result }} {{ windChill.units === 'metric' ? '°C' : '°F' }}</span>
                  </div>
                  <p v-if="windChill.error" class="cipher-error">{{ windChill.error }}</p>
                </div>
              </div>
            </div>

            <!-- Heat Index -->
            <div id="heatindex" class="converter-card">
              <div class="card-header"><h3>{{ t('tools.converters.heatindex') }}</h3></div>
              <div class="converter-body">
                <div class="cipher-info">{{ t('tools.heatindex.info') }}</div>
                <div class="cipher-input-row">
                  <label class="tool-label">
                    {{ t('tools.temperature') }}
                    <input v-model.number="heatIndex.temp" type="number" step="any" class="tool-input" :placeholder="t('tools.example', { value: 35 })" @input="calcHeatIndex" />
                  </label>
                  <label class="tool-label">
                    {{ t('tools.heatindex.humidity') }}
                    <input v-model.number="heatIndex.humidity" type="number" step="any" min="0" max="100" class="tool-input" :placeholder="t('tools.example', { value: 70 })" @input="calcHeatIndex" />
                  </label>
                  <label class="tool-label">
                    {{ t('tools.units.label') }}
                    <select v-model="heatIndex.units" class="tool-select" @change="calcHeatIndex">
                      <option value="metric">°C</option>
                      <option value="imperial">°F</option>
                    </select>
                  </label>
                </div>
                <div class="converter-result-row">
                  <div class="tool-result" v-if="heatIndex.result !== null">
                    <span class="result-label">{{ t('tools.feelsLike') }}</span>
                    <span class="result-value">{{ heatIndex.result }} {{ heatIndex.units === 'metric' ? '°C' : '°F' }}</span>
                  </div>
                  <p v-if="heatIndex.error" class="cipher-error">{{ heatIndex.error }}</p>
                </div>
              </div>
            </div>

          </div>
        </template>

        <!-- Text & Numbers Panel -->
        <template v-if="activeTab === 'text'">
          <div class="panel-header">
            <h2>{{ t('tools.textNumberTools') }}</h2>
          </div>

          <!-- Word / Character Counter -->
          <div v-if="activeTextTool === 'wordcount'" class="cipher-card text-card">
            <div class="card-header"><h3>{{ t('tools.text.wordcount') }}</h3></div>
            <div class="cipher-body">
              <label class="tool-label tool-label-wide">
                {{ t('tools.wordcount.text') }}
                <textarea v-model="textTools_state.wordcount.text" class="tool-textarea" rows="6"
                  :placeholder="t('tools.wordcount.placeholder')" @input="calcWordCount"></textarea>
              </label>
              <div class="stat-grid">
                <div class="stat-cell"><span class="stat-num">{{ textTools_state.wordcount.chars }}</span><span class="stat-lbl">{{ t('tools.wordcount.chars') }}</span></div>
                <div class="stat-cell"><span class="stat-num">{{ textTools_state.wordcount.charsNoSpace }}</span><span class="stat-lbl">{{ t('tools.wordcount.charsNoSpace') }}</span></div>
                <div class="stat-cell"><span class="stat-num">{{ textTools_state.wordcount.words }}</span><span class="stat-lbl">{{ t('tools.wordcount.words') }}</span></div>
                <div class="stat-cell"><span class="stat-num">{{ textTools_state.wordcount.lines }}</span><span class="stat-lbl">{{ t('tools.wordcount.lines') }}</span></div>
                <div class="stat-cell"><span class="stat-num">{{ textTools_state.wordcount.sentences }}</span><span class="stat-lbl">{{ t('tools.wordcount.sentences') }}</span></div>
                <div class="stat-cell"><span class="stat-num">{{ textTools_state.wordcount.paragraphs }}</span><span class="stat-lbl">{{ t('tools.wordcount.paragraphs') }}</span></div>
              </div>
            </div>
          </div>

          <!-- Case Converter -->
          <div v-if="activeTextTool === 'caseconv'" class="cipher-card text-card">
            <div class="card-header"><h3>{{ t('tools.text.caseconv') }}</h3></div>
            <div class="cipher-body">
              <label class="tool-label tool-label-wide">
                {{ t('tools.input') }}
                <textarea v-model="textTools_state.caseconv.text" class="tool-textarea" rows="4"
                  :placeholder="t('tools.caseconv.placeholder')"></textarea>
              </label>
              <div class="cipher-input-row">
                <button class="btn btn-secondary" @click="applyCase('upper')">{{ t('tools.caseconv.upper') }}</button>
                <button class="btn btn-secondary" @click="applyCase('lower')">{{ t('tools.caseconv.lower') }}</button>
                <button class="btn btn-secondary" @click="applyCase('title')">{{ t('tools.caseconv.title') }}</button>
                <button class="btn btn-secondary" @click="applyCase('sentence')">{{ t('tools.caseconv.sentence') }}</button>
                <button class="btn btn-secondary" @click="applyCase('camel')">camelCase</button>
                <button class="btn btn-secondary" @click="applyCase('snake')">snake_case</button>
                <button class="btn btn-secondary" @click="applyCase('kebab')">kebab-case</button>
              </div>
              <div class="cipher-result-row" v-if="textTools_state.caseconv.result !== null">
                <div class="tool-result">
                  <span class="result-label">{{ t('tools.result') }}</span>
                  <span class="result-code">{{ textTools_state.caseconv.result }}</span>
                </div>
              </div>
            </div>
          </div>

          <!-- URL Encode / Decode -->
          <div v-if="activeTextTool === 'urlencode'" class="cipher-card text-card">
            <div class="card-header"><h3>{{ t('tools.text.urlencode') }}</h3></div>
            <div class="cipher-body">
              <div class="cipher-input-row">
                <label class="tool-label">
                  {{ t('tools.mode') }}
                  <select v-model="textTools_state.urlencode.mode" class="tool-select">
                    <option value="encode">{{ t('tools.encode') }}</option>
                    <option value="decode">{{ t('tools.decode') }}</option>
                  </select>
                </label>
                <label class="tool-label tool-label-wide">
                  {{ textTools_state.urlencode.mode === 'encode' ? t('tools.urlencode.plain') : t('tools.urlencode.encoded') }}
                  <textarea v-model="textTools_state.urlencode.text" class="tool-textarea" rows="3"
                    :placeholder="textTools_state.urlencode.mode === 'encode' ? 'https://example.com/path?q=hello world' : t('tools.urlencode.encodedPlaceholder')"
                    @input="handleUrlEncode"></textarea>
                </label>
              </div>
              <div class="cipher-result-row">
                <div class="tool-result" v-if="textTools_state.urlencode.result !== null">
                  <span class="result-label">{{ t('tools.result') }}</span>
                  <code class="result-code">{{ textTools_state.urlencode.result }}</code>
                </div>
                <p v-if="textTools_state.urlencode.error" class="cipher-error">{{ textTools_state.urlencode.error }}</p>
              </div>
            </div>
          </div>

          <!-- Number Base Converter -->
          <div v-if="activeTextTool === 'baseconv'" class="cipher-card text-card">
            <div class="card-header"><h3>{{ t('tools.text.baseconv') }}</h3></div>
            <div class="cipher-body">
              <div class="cipher-input-row">
                <label class="tool-label">
                  {{ t('tools.baseconv.input') }}
                  <input v-model="textTools_state.baseconv.value" type="text" class="tool-input"
                    :placeholder="t('tools.baseconv.placeholder')" @input="handleBaseConv" />
                </label>
                <label class="tool-label">
                  {{ t('tools.baseconv.from') }}
                  <select v-model="textTools_state.baseconv.from" class="tool-select" @change="handleBaseConv">
                    <option value="2">{{ t('tools.baseconv.binary') }} (2)</option>
                    <option value="8">{{ t('tools.baseconv.octal') }} (8)</option>
                    <option value="10">{{ t('tools.baseconv.decimal') }} (10)</option>
                    <option value="16">{{ t('tools.baseconv.hexadecimal') }} (16)</option>
                  </select>
                </label>
              </div>
              <div class="cipher-result-row">
                <div class="tool-result" v-if="textTools_state.baseconv.results !== null">
                  <div class="base-results">
                    <div class="base-row"><span class="base-lbl">{{ t('tools.baseconv.binary') }}</span><code class="result-code">{{ textTools_state.baseconv.results.bin }}</code></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.baseconv.octal') }}</span><code class="result-code">{{ textTools_state.baseconv.results.oct }}</code></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.baseconv.decimal') }}</span><code class="result-code">{{ textTools_state.baseconv.results.dec }}</code></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.baseconv.hex') }}</span><code class="result-code">{{ textTools_state.baseconv.results.hex }}</code></div>
                  </div>
                </div>
                <p v-if="textTools_state.baseconv.error" class="cipher-error">{{ textTools_state.baseconv.error }}</p>
              </div>
            </div>
          </div>

          <!-- ASCII Table -->
          <div v-if="activeTextTool === 'ascii'" class="cipher-card text-card">
            <div class="card-header"><h3>{{ t('tools.text.ascii') }}</h3></div>
            <div class="cipher-body">
              <div class="cipher-input-row">
                <label class="tool-label">
                  {{ t('tools.ascii.input') }}
                  <input v-model="textTools_state.ascii.value" type="text" class="tool-input"
                    :placeholder="t('tools.ascii.placeholder')" @input="handleAsciiLookup" />
                </label>
              </div>
              <div class="cipher-result-row">
                <div class="tool-result" v-if="textTools_state.ascii.result !== null">
                  <div class="base-results">
                    <div class="base-row"><span class="base-lbl">{{ t('tools.ascii.character') }}</span><code class="result-code">{{ textTools_state.ascii.result.char }}</code></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.baseconv.decimal') }}</span><code class="result-code">{{ textTools_state.ascii.result.dec }}</code></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.baseconv.hex') }}</span><code class="result-code">{{ textTools_state.ascii.result.hex }}</code></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.baseconv.binary') }}</span><code class="result-code">{{ textTools_state.ascii.result.bin }}</code></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.baseconv.octal') }}</span><code class="result-code">{{ textTools_state.ascii.result.oct }}</code></div>
                  </div>
                </div>
                <p v-if="textTools_state.ascii.error" class="cipher-error">{{ textTools_state.ascii.error }}</p>
              </div>
            </div>
          </div>
        </template>

        <!-- Date & Time Panel -->
        <template v-if="activeTab === 'datetime'">
          <div class="panel-header">
            <h2>{{ t('tools.datetimeTools') }}</h2>
          </div>

          <!-- Unix Timestamp -->
          <div v-if="activeDatetimeTool === 'unix'" class="cipher-card">
            <div class="card-header"><h3>{{ t('tools.unix.title') }}</h3></div>
            <div class="cipher-body">
              <div class="cipher-input-row">
                <button class="btn btn-secondary" @click="setNowTimestamp">{{ t('tools.unix.now') }}</button>
              </div>
              <div class="cipher-input-row">
                <label class="tool-label">
                  {{ t('tools.unix.seconds') }}
                  <input v-model="dt.unix.ts" type="number" step="1" class="tool-input"
                    :placeholder="t('tools.example', { value: '1700000000' })" @input="unixToHuman" />
                </label>
              </div>
              <div class="cipher-result-row" v-if="dt.unix.human">
                <div class="tool-result">
                  <div class="base-results">
                    <div class="base-row"><span class="base-lbl">UTC</span><code class="result-code">{{ dt.unix.human.utc }}</code></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.unix.local') }}</span><code class="result-code">{{ dt.unix.human.local }}</code></div>
                    <div class="base-row"><span class="base-lbl">ISO 8601</span><code class="result-code">{{ dt.unix.human.iso }}</code></div>
                  </div>
                </div>
              </div>
              <div class="cipher-input-row" style="margin-top: 0.5rem;">
                <label class="tool-label tool-label-wide">
                  {{ t('tools.unix.datestr') }}
                  <input v-model="dt.unix.datestr" type="datetime-local" class="tool-input" style="width:auto" @input="humanToUnix" />
                </label>
              </div>
              <div class="cipher-result-row" v-if="dt.unix.tsOut !== null">
                <div class="tool-result">
                  <span class="result-label">{{ t('tools.unix.result') }}</span>
                  <span class="result-value">{{ dt.unix.tsOut }}</span>
                </div>
              </div>
            </div>
          </div>

          <!-- Duration Calculator -->
          <div v-if="activeDatetimeTool === 'duration'" class="cipher-card">
            <div class="card-header"><h3>{{ t('tools.datetime.duration') }}</h3></div>
            <div class="cipher-body">
              <div class="cipher-input-row">
                <label class="tool-label">
                  {{ t('tools.duration.start') }}
                  <input v-model="dt.duration.start" type="datetime-local" class="tool-input" style="width:auto" @input="calcDuration" />
                </label>
                <label class="tool-label">
                  {{ t('tools.duration.end') }}
                  <input v-model="dt.duration.end" type="datetime-local" class="tool-input" style="width:auto" @input="calcDuration" />
                </label>
              </div>
              <div class="cipher-result-row" v-if="dt.duration.result">
                <div class="tool-result">
                  <div class="base-results">
                    <div class="base-row"><span class="base-lbl">{{ t('tools.duration.days') }}</span><span class="result-value">{{ dt.duration.result.days }}</span></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.duration.hours') }}</span><span class="result-value">{{ dt.duration.result.hours }}</span></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.duration.minutes') }}</span><span class="result-value">{{ dt.duration.result.minutes }}</span></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.duration.seconds') }}</span><span class="result-value">{{ dt.duration.result.seconds }}</span></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.duration.human') }}</span><span class="result-value">{{ dt.duration.result.human }}</span></div>
                  </div>
                </div>
              </div>
            </div>
          </div>

          <!-- Day of Year / Week -->
          <div v-if="activeDatetimeTool === 'dayofyear'" class="cipher-card">
            <div class="card-header"><h3>{{ t('tools.dayofyear.title') }}</h3></div>
            <div class="cipher-body">
              <div class="cipher-input-row">
                <label class="tool-label">
                  {{ t('tools.dayofyear.date') }}
                  <input v-model="dt.dayofyear.date" type="date" class="tool-input" style="width:auto" @input="calcDayOfYear" />
                </label>
                <button class="btn btn-secondary" @click="setTodayDayOfYear">{{ t('time.today') }}</button>
              </div>
              <div class="cipher-result-row" v-if="dt.dayofyear.result">
                <div class="tool-result">
                  <div class="base-results">
                    <div class="base-row"><span class="base-lbl">{{ t('tools.dayofyear.doy') }}</span><span class="result-value">{{ dt.dayofyear.result.doy }}</span></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.dayofyear.week') }}</span><span class="result-value">{{ dt.dayofyear.result.week }}</span></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.dayofyear.dow') }}</span><span class="result-value">{{ dt.dayofyear.result.dow }}</span></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.dayofyear.daysLeft') }}</span><span class="result-value">{{ dt.dayofyear.result.daysLeft }}</span></div>
                    <div class="base-row"><span class="base-lbl">{{ t('tools.dayofyear.leap') }}</span><span class="result-value">{{ dt.dayofyear.result.leap ? t('common.yes') : t('common.no') }}</span></div>
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
                  {{ t('tools.mode') }}
                  <select v-model="ciphers.aes.mode" class="tool-select">
                    <option value="encrypt">{{ t('tools.aes.encrypt') }}</option>
                    <option value="decrypt">{{ t('tools.aes.decrypt') }}</option>
                  </select>
                </label>
                <label class="tool-label">
                  {{ t('tools.aes.keySource') }}
                  <select v-model="ciphers.aes.key_type" class="tool-select">
                    <option value="password">{{ t('tools.aes.password') }} (PBKDF2)</option>
                    <option value="raw">{{ t('tools.aes.rawKey') }}</option>
                  </select>
                </label>
                <label class="tool-label">
                  {{ t('tools.aes.bits') }}
                  <select v-model="ciphers.aes.bits" class="tool-select">
                    <option value="128">128</option>
                    <option value="192">192</option>
                    <option value="256">256</option>
                  </select>
                </label>
                <label class="tool-label">
                  {{ t('tools.aes.cipherMode') }}
                  <select v-model="ciphers.aes.cipher_mode" class="tool-select">
                    <option value="gcm">GCM</option>
                    <option value="cbc">CBC</option>
                    <option value="ecb">ECB</option>
                  </select>
                </label>
              </div>

              <div class="cipher-info" v-if="ciphers.aes.key_type === 'password'">
                <i18n-t keypath="tools.aes.kdfInfo" scope="global">
                  <template #kdf><strong>{{ t('tools.aes.kdf', { n: n(100000) }) }}</strong></template>
                </i18n-t>
              </div>
              <div class="cipher-info cipher-warning" v-if="ciphers.aes.cipher_mode === 'ecb'">
                ⚠ {{ t('tools.aes.ecbWarning') }}
              </div>
              <div class="cipher-info cipher-warning" v-if="ciphers.aes.cipher_mode === 'gcm' && ciphers.aes.bits !== '256'">
                ⚠ {{ t('tools.aes.gcmWarning') }}
              </div>

              <div class="cipher-input-row">
                <label class="tool-label">
                  {{ ciphers.aes.key_type === 'password' ? t('tools.aes.password') : t('tools.aes.keyHex') }}
                  <input v-model="ciphers.aes.key_source" type="text" class="tool-input"
                    :placeholder="ciphers.aes.key_type === 'password' ? t('tools.aes.passwordPlaceholder') : t('tools.aes.keyPlaceholder')" />
                </label>
                <label class="tool-label tool-label-wide">
                  {{ ciphers.aes.mode === 'encrypt' ? t('tools.plaintext') : t('tools.aes.ciphertext') }}
                  <textarea v-model="ciphers.aes.text" class="tool-textarea" rows="4"
                    :placeholder="ciphers.aes.mode === 'encrypt' ? t('tools.aes.encryptPlaceholder') : t('tools.aes.decryptPlaceholder')"></textarea>
                </label>
                <div class="cipher-actions">
                  <button class="btn btn-primary" @click="handleAes" :disabled="cipherWorking">
                    {{ ciphers.aes.mode === 'encrypt' ? t('tools.aes.encrypt') : t('tools.aes.decrypt') }}
                  </button>
                </div>
              </div>
              <div class="cipher-result-row">
                <div class="tool-result" v-if="ciphers.aes.result !== null">
                  <span class="result-label">{{ t('tools.result') }}</span>
                  <code class="result-code">{{ ciphers.aes.result }}</code>
                </div>
                <p v-if="ciphers.aes.error" class="cipher-error">{{ ciphers.aes.error }}</p>
              </div>
            </div>
          </div>

          <!-- Base64 -->
          <div v-if="activeCipher === 'base64'" class="cipher-card">
            <div class="card-header"><h3>Base64</h3></div>
            <div class="cipher-body">
              <div class="cipher-input-row">
                <label class="tool-label">
                  {{ t('tools.mode') }}
                  <select v-model="ciphers.base64.mode" class="tool-select">
                    <option value="encode">{{ t('tools.encode') }}</option>
                    <option value="decode">{{ t('tools.decode') }}</option>
                  </select>
                </label>
                <label class="tool-label tool-label-wide">
                  {{ ciphers.base64.mode === 'encode' ? t('tools.plaintext') : t('tools.base64.string') }}
                  <textarea v-model="ciphers.base64.text" class="tool-textarea" rows="4"
                    :placeholder="ciphers.base64.mode === 'encode' ? t('tools.base64.encodePlaceholder') : t('tools.base64.decodePlaceholder')"
                    @input="handleBase64"></textarea>
                </label>
              </div>
              <div class="cipher-result-row">
                <div class="tool-result" v-if="ciphers.base64.result !== null">
                  <span class="result-label">{{ t('tools.result') }}</span>
                  <code class="result-code">{{ ciphers.base64.result }}</code>
                </div>
                <p v-if="ciphers.base64.error" class="cipher-error">{{ ciphers.base64.error }}</p>
              </div>
            </div>
          </div>

          <!-- ROT13 -->
          <div v-if="activeCipher === 'rot13'" class="cipher-card">
            <div class="card-header"><h3>ROT13</h3></div>
            <div class="cipher-body">
              <div class="cipher-input-row">
                <label class="tool-label tool-label-wide">
                  {{ t('tools.input') }}
                  <textarea v-model="ciphers.rot13.text" class="tool-textarea" rows="4"
                    :placeholder="t('tools.rot13.placeholder')" @input="handleRot13"></textarea>
                </label>
              </div>
              <div class="cipher-result-row">
                <div class="tool-result" v-if="ciphers.rot13.result !== null">
                  <span class="result-label">{{ t('tools.result') }}</span>
                  <code class="result-code">{{ ciphers.rot13.result }}</code>
                </div>
              </div>
            </div>
          </div>

          <!-- Morse Code -->
          <div v-if="activeCipher === 'morse'" class="cipher-card">
            <div class="card-header"><h3>{{ t('tools.ciphers.morse') }}</h3></div>
            <div class="cipher-body">
              <div class="cipher-input-row">
                <label class="tool-label">
                  {{ t('tools.mode') }}
                  <select v-model="ciphers.morse.mode" class="tool-select">
                    <option value="encode">{{ t('tools.morse.toMorse') }}</option>
                    <option value="decode">{{ t('tools.morse.toText') }}</option>
                  </select>
                </label>
                <label class="tool-label tool-label-wide">
                  {{ ciphers.morse.mode === 'encode' ? t('tools.morse.textLabel') : t('tools.morse.codeLabel') }}
                  <textarea v-model="ciphers.morse.text" class="tool-textarea" rows="4"
                    :placeholder="ciphers.morse.mode === 'encode' ? 'HELLO WORLD' : '.... . .-.. .-.. --- / .-- --- .-. .-.. -..'"
                    @input="handleMorse"></textarea>
                </label>
              </div>
              <div class="cipher-result-row">
                <div class="tool-result" v-if="ciphers.morse.result !== null">
                  <span class="result-label">{{ t('tools.result') }}</span>
                  <code class="result-code">{{ ciphers.morse.result }}</code>
                </div>
                <p v-if="ciphers.morse.error" class="cipher-error">{{ ciphers.morse.error }}</p>
              </div>
            </div>
          </div>

          <!-- Hash / Checksum -->
          <div v-if="activeCipher === 'hash'" class="cipher-card">
            <div class="card-header"><h3>{{ t('tools.ciphers.hash') }}</h3></div>
            <div class="cipher-body">
              <div class="cipher-input-row">
                <label class="tool-label">
                  {{ t('tools.hash.algorithm') }}
                  <select v-model="ciphers.hash.algo" class="tool-select" @change="handleHash">
                    <option value="sha256">SHA-256</option>
                    <option value="sha512">SHA-512</option>
                    <option value="sha1">SHA-1</option>
                    <option value="md5">MD5</option>
                  </select>
                </label>
                <label class="tool-label tool-label-wide">
                  {{ t('tools.input') }}
                  <textarea v-model="ciphers.hash.text" class="tool-textarea" rows="4"
                    :placeholder="t('tools.hash.placeholder')" @input="handleHash"></textarea>
                </label>
              </div>
              <div class="cipher-result-row">
                <div class="tool-result" v-if="ciphers.hash.result !== null">
                  <span class="result-label">{{ t('tools.hash.result') }}</span>
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
import { useI18n } from 'vue-i18n'
import { apiService } from '../services/api.js'

const { t, n, locale } = useI18n()
// Some calculators use a local `t` for temperature.
const tr = t
const sidebarCollapsed = ref(false)
const activeTab = ref('converters')
const activeConverter = ref('length')
const activeCipher = ref('aes')
const activeTextTool = ref('wordcount')
const activeDatetimeTool = ref('unix')
const cipherWorking = ref(false)

// Converter groups and tools carry message keys; templates translate them.
const groupLabelKey = (groupKey) => `tools.groups.${groupKey}`

const converterGroups = {
  length_speed: [
    { id: 'length', labelKey: 'tools.converters.length' },
    { id: 'speed', labelKey: 'tools.converters.speed' }
  ],
  weight_volume: [
    { id: 'mass', labelKey: 'tools.converters.mass' },
    { id: 'volume', labelKey: 'tools.converters.volume' }
  ],
  area_angle: [
    { id: 'area', labelKey: 'tools.converters.area' },
    { id: 'angle', labelKey: 'tools.converters.angle' }
  ],
  temperature: [
    { id: 'temperature', labelKey: 'tools.temperature' }
  ],
  digital: [
    { id: 'data', labelKey: 'tools.converters.data' }
  ],
  energy_power: [
    { id: 'energy', labelKey: 'tools.converters.energy' },
    { id: 'power', labelKey: 'tools.converters.power' }
  ],
  pressure_time: [
    { id: 'pressure', labelKey: 'tools.converters.pressure' },
    { id: 'time', labelKey: 'tools.converters.time' }
  ],
  nav_weather: [
    { id: 'coordinates', labelKey: 'tools.converters.coordinates' },
    { id: 'windchill', labelKey: 'tools.converters.windchill' },
    { id: 'heatindex', labelKey: 'tools.converters.heatindex' }
  ]
}

const cipherTools = [
  { id: 'aes', labelKey: 'tools.ciphers.aes' },
  { id: 'base64', labelKey: 'tools.ciphers.base64' },
  { id: 'rot13', labelKey: 'tools.ciphers.rot13' },
  { id: 'morse', labelKey: 'tools.ciphers.morse' },
  { id: 'hash', labelKey: 'tools.ciphers.hash' }
]

const textTools = [
  { id: 'wordcount', labelKey: 'tools.text.wordcount' },
  { id: 'caseconv', labelKey: 'tools.text.caseconv' },
  { id: 'urlencode', labelKey: 'tools.text.urlencode' },
  { id: 'ascii', labelKey: 'tools.text.ascii' }
]

const numberTools = [
  { id: 'baseconv', labelKey: 'tools.text.baseconv' }
]

const datetimeTools = [
  { id: 'unix', labelKey: 'tools.datetime.unix' },
  { id: 'duration', labelKey: 'tools.datetime.duration' },
  { id: 'dayofyear', labelKey: 'tools.datetime.dayofyear' }
]

const currentCipherLabel = computed(() => {
  const tool = cipherTools.find(t => t.id === activeCipher.value)
  return tool ? t(tool.labelKey) : t('tools.cipher')
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
  length: ['mm', 'cm', 'm', 'km', 'in', 'ft', 'yd', 'mi', 'nmi'],
  mass: ['mg', 'g', 'kg', 't', 'oz', 'lb', 'st'],
  temperature: ['C', 'F', 'K'],
  area: ['mm²', 'cm²', 'm²', 'km²', 'ha', 'in²', 'ft²', 'ac'],
  volume: ['mL', 'L', 'm³', 'fl_oz', 'gal', 'cup', 'tbsp', 'tsp'],
  speed: ['m/s', 'km/h', 'mph', 'knot', 'ft/s'],
  data: ['B', 'KB', 'MB', 'GB', 'TB', 'PB', 'KiB', 'MiB', 'GiB', 'TiB'],
  angle: ['deg', 'rad', 'grad'],
  pressure: ['Pa', 'kPa', 'MPa', 'bar', 'mbar', 'psi', 'atm', 'mmHg', 'inHg'],
  energy: ['J', 'kJ', 'cal', 'kcal', 'Wh', 'kWh', 'BTU'],
  power: ['W', 'kW', 'MW', 'HP', 'BTU/h'],
  time: ['ms', 's', 'min', 'h', 'day', 'week']
}

function getUnits(catId) {
  return unitSets[catId] || []
}

// Unit symbols are international; only word-like units are translated.
const TRANSLATED_UNITS = { day: 'day', week: 'week', knot: 'knot', cup: 'cup', tbsp: 'tbsp', tsp: 'tsp', fl_oz: 'flOz', gal: 'gal', deg: 'deg' }
function unitLabel(unit) {
  return TRANSLATED_UNITS[unit] ? t(`tools.units.${TRANSLATED_UNITS[unit]}`) : unit
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

const converterCardGroups = computed(() => {
  return Object.entries(converterGroups)
    .filter(([groupKey]) => groupKey !== 'nav_weather')
    .map(([groupKey, group]) => ({
      key: groupKey,
      labelKey: groupLabelKey(groupKey),
      categories: group.filter((cat) => Boolean(converters[cat.id]))
    }))
})

// --- Cipher state ---
const ciphers = reactive({
  aes: { mode: 'encrypt', key_type: 'password', key_source: '', bits: '256', cipher_mode: 'gcm', text: '', result: null, error: null },
  base64: { mode: 'encode', text: '', result: null, error: null },
  rot13: { text: '', result: null },
  morse: { mode: 'encode', text: '', result: null, error: null },
  hash: { algo: 'sha256', text: '', result: null }
})

// --- Text tools state ---
const textTools_state = reactive({
  wordcount: { text: '', chars: 0, charsNoSpace: 0, words: 0, lines: 0, sentences: 0, paragraphs: 0 },
  caseconv: { text: '', result: null },
  urlencode: { mode: 'encode', text: '', result: null, error: null },
  baseconv: { value: '', from: '10', results: null, error: null },
  ascii: { value: '', result: null, error: null }
})

// --- Date/time state ---
const dt = reactive({
  unix: { ts: null, human: null, datestr: '', tsOut: null },
  duration: { start: '', end: '', result: null },
  dayofyear: { date: '', result: null }
})

// --- Coordinate state ---
const coordMode = ref('to_dms')
const coordDec = reactive({ lat: null, lon: null })
const coordDms = reactive({ latD: null, latM: null, latS: null, latDir: 'N', lonD: null, lonM: null, lonS: null, lonDir: 'E' })
const coordResult = ref(null)

// --- Wind Chill state ---
const windChill = reactive({ temp: null, wind: null, units: 'metric', result: null, error: null })

// --- Heat Index state ---
const heatIndex = reactive({ temp: null, humidity: null, units: 'metric', result: null, error: null })

// --- Conversion tables ---
const LENGTH_TO_M = {
  mm: 0.001, cm: 0.01, m: 1, km: 1000,
  in: 0.0254, ft: 0.3048, yd: 0.9144, mi: 1609.344, nmi: 1852
}

const MASS_TO_KG = {
  mg: 0.000001, g: 0.001, kg: 1, t: 1000, oz: 0.0283495, lb: 0.453592, st: 6.35029
}

const AREA_TO_M2 = {
  'mm²': 0.000001, 'cm²': 0.0001, 'm²': 1, 'km²': 1000000,
  ha: 10000, 'in²': 0.00064516, 'ft²': 0.092903, ac: 4046.86
}

const VOLUME_TO_L = {
  mL: 0.001, L: 1, 'm³': 1000, fl_oz: 0.0295735, gal: 3.78541, cup: 0.236588, tbsp: 0.0147868, tsp: 0.00492892
}

const SPEED_TO_MS = {
  'm/s': 1, 'km/h': 0.277778, mph: 0.44704, knot: 0.514444, 'ft/s': 0.3048
}

const DATA_TO_B = {
  B: 1, KB: 1000, MB: 1000000, GB: 1000000000, TB: 1000000000000, PB: 1000000000000000,
  KiB: 1024, MiB: 1048576, GiB: 1073741824, TiB: 1099511627776
}

const ANGLE_TO_DEG = {
  deg: 1, rad: 180 / Math.PI, grad: 0.9
}

const PRESSURE_TO_PA = {
  Pa: 1, kPa: 1000, MPa: 1000000, bar: 100000, mbar: 100,
  psi: 6894.76, atm: 101325, mmHg: 133.322, inHg: 3386.39
}

const ENERGY_TO_J = {
  J: 1, kJ: 1000, cal: 4.184, kcal: 4184, Wh: 3600, kWh: 3600000, BTU: 1055.06
}

const POWER_TO_W = {
  W: 1, kW: 1000, MW: 1000000, HP: 745.7, 'BTU/h': 0.293071
}

const TIME_TO_S = {
  ms: 0.001, s: 1, min: 60, h: 3600, day: 86400, week: 604800
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
  if (!state) return
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

// --- Coordinate converter ---
function decToDms(decimal) {
  const sign = decimal < 0 ? -1 : 1
  const abs = Math.abs(decimal)
  const d = Math.floor(abs)
  const minFull = (abs - d) * 60
  const m = Math.floor(minFull)
  const s = (minFull - m) * 60
  return { d: d * sign, m, s: Math.round(s * 1000) / 1000 }
}

function dmsToDec(d, m, s, dir) {
  const dec = Math.abs(d) + m / 60 + s / 3600
  return (dir === 'S' || dir === 'W') ? -dec : dec
}

function convertCoordToDms() {
  if (coordDec.lat == null || coordDec.lon == null || isNaN(coordDec.lat) || isNaN(coordDec.lon)) {
    coordResult.value = null
    return
  }
  const lat = decToDms(coordDec.lat)
  const lon = decToDms(coordDec.lon)
  const latDir = coordDec.lat >= 0 ? 'N' : 'S'
  const lonDir = coordDec.lon >= 0 ? 'E' : 'W'
  coordResult.value = `${Math.abs(lat.d)}° ${lat.m}' ${lat.s}" ${latDir}   ${Math.abs(lon.d)}° ${lon.m}' ${lon.s}" ${lonDir}`
}

function convertCoordToDecimal() {
  const { latD, latM, latS, latDir, lonD, lonM, lonS, lonDir } = coordDms
  if ([latD, latM, latS, lonD, lonM, lonS].some(v => v == null || isNaN(v))) {
    coordResult.value = null
    return
  }
  const lat = dmsToDec(latD, latM, latS, latDir)
  const lon = dmsToDec(lonD, lonM, lonS, lonDir)
  coordResult.value = t('tools.coord.result', { lat: lat.toFixed(6), lon: lon.toFixed(6) })
}

// --- Wind Chill (Environment Canada / US NWS formula) ---
function calcWindChill() {
  windChill.error = null
  windChill.result = null
  if (windChill.temp == null || windChill.wind == null || isNaN(windChill.temp) || isNaN(windChill.wind)) return

  let t = Number(windChill.temp)
  let v = Number(windChill.wind)

  if (windChill.units === 'imperial') {
    // °F / mph
    if (t > 50) { windChill.error = tr('tools.windchill.tempLimit', { limit: '50°F' }); return }
    if (v < 3) { windChill.error = tr('tools.windchill.windLimit', { limit: '3 mph' }); return }
    windChill.result = Math.round((35.74 + 0.6215 * t - 35.75 * Math.pow(v, 0.16) + 0.4275 * t * Math.pow(v, 0.16)) * 10) / 10
  } else {
    // °C / km/h
    if (t > 10) { windChill.error = tr('tools.windchill.tempLimit', { limit: '10°C' }); return }
    if (v < 4.8) { windChill.error = tr('tools.windchill.windLimit', { limit: '4.8 km/h' }); return }
    windChill.result = Math.round((13.12 + 0.6215 * t - 11.37 * Math.pow(v, 0.16) + 0.3965 * t * Math.pow(v, 0.16)) * 10) / 10
  }
}

// --- Heat Index (Rothfusz regression) ---
function calcHeatIndex() {
  heatIndex.error = null
  heatIndex.result = null
  if (heatIndex.temp == null || heatIndex.humidity == null || isNaN(heatIndex.temp) || isNaN(heatIndex.humidity)) return

  let t = Number(heatIndex.temp)
  const rh = Number(heatIndex.humidity)

  // Convert to Fahrenheit for calculation
  const tF = heatIndex.units === 'metric' ? t * 9 / 5 + 32 : t

  if (tF < 80) { heatIndex.error = tr('tools.heatindex.tempLimit'); return }
  if (rh < 40) { heatIndex.error = tr('tools.heatindex.humidityLimit'); return }

  const hi = -42.379 + 2.04901523 * tF + 10.14333127 * rh
    - 0.22475541 * tF * rh - 0.00683783 * tF * tF
    - 0.05481717 * rh * rh + 0.00122874 * tF * tF * rh
    + 0.00085282 * tF * rh * rh - 0.00000199 * tF * tF * rh * rh

  heatIndex.result = heatIndex.units === 'metric'
    ? Math.round((hi - 32) * 5 / 9 * 10) / 10
    : Math.round(hi * 10) / 10
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
    ciphers.aes.error = msg || t('tools.aes.failed')
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
    ciphers.base64.error = e.message || t('tools.errors.invalidOperation')
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

// --- Morse Code ---
const MORSE_MAP = {
  A: '.-', B: '-...', C: '-.-.', D: '-..', E: '.', F: '..-.', G: '--.', H: '....',
  I: '..', J: '.---', K: '-.-', L: '.-..', M: '--', N: '-.', O: '---', P: '.--.',
  Q: '--.-', R: '.-.', S: '...', T: '-', U: '..-', V: '...-', W: '.--', X: '-..-',
  Y: '-.--', Z: '--..',
  '0': '-----', '1': '.----', '2': '..---', '3': '...--', '4': '....-',
  '5': '.....', '6': '-....', '7': '--...', '8': '---..', '9': '----.',
  '.': '.-.-.-', ',': '--..--', '?': '..--..', "'": '.----.', '!': '-.-.--',
  '/': '-..-.', '(': '-.--.', ')': '-.--.-', '&': '.-...', ':': '---...',
  ';': '-.-.-.', '=': '-...-', '+': '.-.-.', '-': '-....-', '_': '..--.-',
  '"': '.-..-.', '$': '...-..-', '@': '.--.-.'
}
const MORSE_REVERSE = Object.fromEntries(Object.entries(MORSE_MAP).map(([k, v]) => [v, k]))

function handleMorse() {
  ciphers.morse.error = null
  ciphers.morse.result = null
  if (!ciphers.morse.text) return

  try {
    if (ciphers.morse.mode === 'encode') {
      const words = ciphers.morse.text.toUpperCase().split(/\s+/)
      ciphers.morse.result = words.map(word =>
        word.split('').map(ch => MORSE_MAP[ch] || '?').join(' ')
      ).join(' / ')
    } else {
      const words = ciphers.morse.text.trim().split(/\s*\/\s*/)
      ciphers.morse.result = words.map(word =>
        word.trim().split(/\s+/).map(code => {
          const ch = MORSE_REVERSE[code]
          if (!ch) throw new Error(t('tools.morse.unknown', { code }))
          return ch
        }).join('')
      ).join(' ')
    }
  } catch (e) {
    ciphers.morse.error = e.message || t('tools.morse.invalid')
    ciphers.morse.result = null
  }
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

// --- Text tools ---
function calcWordCount() {
  const t = textTools_state.wordcount.text
  textTools_state.wordcount.chars = t.length
  textTools_state.wordcount.charsNoSpace = t.replace(/\s/g, '').length
  textTools_state.wordcount.words = t.trim() === '' ? 0 : t.trim().split(/\s+/).length
  textTools_state.wordcount.lines = t === '' ? 0 : t.split('\n').length
  textTools_state.wordcount.sentences = t.trim() === '' ? 0 : (t.match(/[.!?]+/g) || []).length
  textTools_state.wordcount.paragraphs = t.trim() === '' ? 0 : t.split(/\n\s*\n/).filter(p => p.trim()).length || (t.trim() ? 1 : 0)
}

function toTitleCase(str) {
  return str.replace(/\w\S*/g, txt => txt.charAt(0).toUpperCase() + txt.substr(1).toLowerCase())
}

function toSentenceCase(str) {
  return str.toLowerCase().replace(/(^\s*\w|[.!?]\s*\w)/g, c => c.toUpperCase())
}

function toCamelCase(str) {
  return str.toLowerCase().replace(/[^a-zA-Z0-9]+(.)/g, (_, c) => c.toUpperCase())
}

function toSnakeCase(str) {
  return str.trim().toLowerCase().replace(/[^a-zA-Z0-9]+/g, '_').replace(/^_|_$/g, '')
}

function toKebabCase(str) {
  return str.trim().toLowerCase().replace(/[^a-zA-Z0-9]+/g, '-').replace(/^-|-$/g, '')
}

function applyCase(type) {
  const text = textTools_state.caseconv.text
  const map = {
    upper: () => text.toUpperCase(),
    lower: () => text.toLowerCase(),
    title: () => toTitleCase(text),
    sentence: () => toSentenceCase(text),
    camel: () => toCamelCase(text),
    snake: () => toSnakeCase(text),
    kebab: () => toKebabCase(text)
  }
  textTools_state.caseconv.result = map[type] ? map[type]() : text
}

function handleUrlEncode() {
  textTools_state.urlencode.error = null
  if (!textTools_state.urlencode.text) {
    textTools_state.urlencode.result = null
    return
  }
  try {
    if (textTools_state.urlencode.mode === 'encode') {
      textTools_state.urlencode.result = encodeURIComponent(textTools_state.urlencode.text)
    } else {
      textTools_state.urlencode.result = decodeURIComponent(textTools_state.urlencode.text)
    }
  } catch (e) {
    textTools_state.urlencode.error = e.message || t('tools.errors.invalidInput')
    textTools_state.urlencode.result = null
  }
}

function handleBaseConv() {
  textTools_state.baseconv.error = null
  textTools_state.baseconv.results = null
  const val = textTools_state.baseconv.value.trim()
  if (!val) return
  try {
    const fromBase = parseInt(textTools_state.baseconv.from)
    const prefixed = fromBase === 16 ? '0x' + val : fromBase === 8 ? '0o' + val : fromBase === 2 ? '0b' + val : val
    const n = BigInt(fromBase === 10 ? val : prefixed)
    textTools_state.baseconv.results = {
      bin: n.toString(2),
      oct: n.toString(8),
      dec: n.toString(10),
      hex: n.toString(16).toUpperCase()
    }
  } catch (e) {
    textTools_state.baseconv.error = t('tools.baseconv.invalid')
  }
}

function handleAsciiLookup() {
  textTools_state.ascii.error = null
  textTools_state.ascii.result = null
  const v = textTools_state.ascii.value.trim()
  if (!v) return
  let code
  if ([...v].length === 1) {
    code = v.codePointAt(0)
  } else {
    let parsed
    if (/^0x[0-9a-fA-F]+$/.test(v)) parsed = parseInt(v, 16)
    else if (/^0b[01]+$/.test(v)) parsed = parseInt(v.slice(2), 2)
    else if (/^0o[0-7]+$/.test(v)) parsed = parseInt(v.slice(2), 8)
    else parsed = parseInt(v, 10)
    if (isNaN(parsed) || parsed < 0) { textTools_state.ascii.error = t('tools.ascii.invalid'); return }
    code = parsed
  }
  textTools_state.ascii.result = {
    char: String.fromCodePoint(code),
    dec: code,
    hex: '0x' + code.toString(16).toUpperCase().padStart(4, '0'),
    bin: code.toString(2).padStart(8, '0'),
    oct: '0' + code.toString(8)
  }
}

// --- Date/Time tools ---
function setNowTimestamp() {
  dt.unix.ts = Math.floor(Date.now() / 1000)
  unixToHuman()
}

function unixToHuman() {
  if (dt.unix.ts == null || isNaN(dt.unix.ts)) { dt.unix.human = null; return }
  const d = new Date(Number(dt.unix.ts) * 1000)
  dt.unix.human = {
    utc: d.toUTCString(),
    local: d.toLocaleString(locale.value),
    iso: d.toISOString()
  }
}

function humanToUnix() {
  if (!dt.unix.datestr) { dt.unix.tsOut = null; return }
  const d = new Date(dt.unix.datestr)
  dt.unix.tsOut = isNaN(d.getTime()) ? null : Math.floor(d.getTime() / 1000)
}

function calcDuration() {
  if (!dt.duration.start || !dt.duration.end) { dt.duration.result = null; return }
  const s = new Date(dt.duration.start).getTime()
  const e = new Date(dt.duration.end).getTime()
  if (isNaN(s) || isNaN(e)) { dt.duration.result = null; return }
  let diff = Math.abs(e - s) / 1000
  const days = Math.floor(diff / 86400); diff -= days * 86400
  const hours = Math.floor(diff / 3600); diff -= hours * 3600
  const minutes = Math.floor(diff / 60)
  const seconds = Math.floor(diff - minutes * 60)
  const parts = []
  if (days) parts.push(t('tools.duration.d', { n: days }))
  if (hours) parts.push(t('tools.duration.h', { n: hours }))
  if (minutes) parts.push(t('tools.duration.m', { n: minutes }))
  if (seconds || !parts.length) parts.push(t('tools.duration.s', { n: seconds }))
  dt.duration.result = { days, hours, minutes, seconds, human: parts.join(' ') }
}

function setTodayDayOfYear() {
  dt.dayofyear.date = new Date().toISOString().slice(0, 10)
  calcDayOfYear()
}

function calcDayOfYear() {
  if (!dt.dayofyear.date) { dt.dayofyear.result = null; return }
  const d = new Date(dt.dayofyear.date + 'T00:00:00')
  if (isNaN(d.getTime())) { dt.dayofyear.result = null; return }
  const year = d.getFullYear()
  const start = new Date(year, 0, 0)
  const diff = d - start
  const doy = Math.floor(diff / 86400000)
  const isLeap = (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0
  const daysInYear = isLeap ? 366 : 365
  // ISO week number
  const dayOfWeek = (d.getDay() + 6) % 7 // Mon=0 … Sun=6
  const nearestThursday = new Date(d)
  nearestThursday.setDate(d.getDate() - dayOfWeek + 3) // set to Thursday of this week
  const jan4 = new Date(nearestThursday.getFullYear(), 0, 4)
  const startOfWeek1 = new Date(jan4)
  startOfWeek1.setDate(jan4.getDate() - ((jan4.getDay() + 6) % 7))
  const week = Math.round((nearestThursday - startOfWeek1) / (7 * 86400000)) + 1
  dt.dayofyear.result = {
    doy,
    week,
    dow: d.toLocaleDateString(locale.value, { weekday: 'long' }),
    daysLeft: daysInYear - doy,
    leap: isLeap
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
  min-height: 56px;
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
  flex-shrink: 0;
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
  flex-wrap: wrap;
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

.text-card {
  border-left-color: #3da87a;
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
  min-width: 140px;
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

.btn-secondary {
  background: #252d33;
  color: var(--text);
  border: 1px solid var(--line);
}

.btn-secondary:hover {
  background: #2e3840;
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

/* Stat grid for word counter */
.stat-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
  gap: 0.5rem;
}

.stat-cell {
  background: #11161a;
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 0.75rem 1rem;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.2rem;
}

.stat-num {
  color: #667eea;
  font-size: 1.4rem;
  font-weight: 700;
}

.stat-lbl {
  color: var(--muted);
  font-size: 0.75rem;
  text-align: center;
}

/* Base converter results */
.base-results {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  width: 100%;
}

.base-row {
  display: flex;
  align-items: baseline;
  gap: 0.75rem;
}

.base-lbl {
  color: var(--muted);
  font-size: 0.78rem;
  min-width: 90px;
  flex-shrink: 0;
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
