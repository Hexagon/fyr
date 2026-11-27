<template>
  <label class="language-switcher" :class="{ compact }">
    <span :class="compact ? 'visually-hidden' : 'language-label'">{{ t('language.label') }}</span>
    <select
      class="language-select"
      :value="current"
      :title="compact ? t('language.label') : undefined"
      :disabled="switching"
      @change="onChange"
    >
      <option v-for="code in SUPPORTED_LOCALES" :key="code" :value="code" :lang="code">
        {{ compact ? code.toUpperCase() : LOCALE_NAMES[code] }}
      </option>
    </select>
  </label>
</template>

<script setup>
// Language picker. The choice is stored per browser (localStorage), never on
// the server, so guests in password mode can switch too.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { LOCALE_NAMES, SUPPORTED_LOCALES, setLocale } from '../i18n/index.js'

defineProps({
  compact: { type: Boolean, default: false }
})

const { t, locale } = useI18n()
const switching = ref(false)
const current = computed(() => locale.value)

const onChange = async (event) => {
  switching.value = true
  try {
    await setLocale(event.target.value)
  } finally {
    switching.value = false
    // Re-sync in case loading failed and the language did not change.
    event.target.value = locale.value
  }
}
</script>

<style scoped>
.language-switcher {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
}

.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
  white-space: nowrap;
}

.language-label {
  font-weight: 600;
}

.language-select {
  background: rgba(255, 255, 255, 0.07);
  color: inherit;
  border: 1px solid rgba(255, 255, 255, 0.2);
  border-radius: 0.4rem;
  padding: 0.35rem 0.5rem;
  font-size: 0.9rem;
  cursor: pointer;
}

.compact .language-select {
  padding: 0.3rem 0.35rem;
  font-size: 0.78rem;
  font-weight: 600;
  letter-spacing: 0.04em;
}

.language-select option {
  color: #111;
}
</style>
