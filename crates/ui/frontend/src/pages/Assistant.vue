<template>
  <div class="assistant-page">
    <div class="assistant-layout">
      <aside class="model-sidebar" :class="{ collapsed: sidebarCollapsed }">
        <div class="sidebar-header">
          <h3>{{ t('assistant.library') }}</h3>
          <button
            class="icon-btn"
            :aria-label="sidebarCollapsed ? t('assistant.expandLibrary') : t('assistant.collapseLibrary')"
            :title="sidebarCollapsed ? t('common.expandPanel') : t('common.collapsePanel')"
            :aria-expanded="String(!sidebarCollapsed)"
            @click="toggleSidebar"
          >
            {{ sidebarCollapsed ? '»' : '«' }}
          </button>
        </div>

        <div v-if="!sidebarCollapsed" class="sidebar-content">
          <router-link
            v-if="!adminLocked"
            class="btn btn-primary import-link"
            :to="{ name: 'ContentManager', query: { category: 'models' } }"
          >
            {{ t('assistant.openContentManager') }}
          </router-link>
          <router-link
            v-else-if="authState.requiresAuth && !authState.readonly"
            class="btn btn-secondary import-link"
            to="/login"
          >
            {{ t('assistant.loginToManage') }}
          </router-link>
          <p class="hint-text">{{ t('assistant.uploadHint') }}</p>

          <div class="model-list">
            <button
              v-for="model in models"
              :key="model.filename"
              class="model-item"
              :class="{ active: selectedModel?.filename === model.filename }"
              @click="selectModel(model)"
            >
              <span class="model-name">{{ model.filename }}</span>
              <span class="model-meta">{{ formatBytes(model.size_bytes || model.size || 0) }}</span>
            </button>
            <p v-if="!models.length" class="empty-state">{{ t('assistant.noModels') }}</p>
          </div>
        </div>
      </aside>

      <section class="chat-panel">
        <template v-if="hasLoadedModel">
          <div class="status-row">
            <p class="status-model loaded">
              {{ modelStatusText }}
            </p>
          </div>

          <div ref="chatHistoryRef" class="chat-history">
            <div v-for="message in messages" :key="message.id" class="bubble" :class="[message.role, { streaming: message.streaming }]">
              <div
                v-if="message.role === 'assistant'"
                class="bubble-content markdown-content"
              >
                <details v-if="shouldShowThinkingBlock(message)" class="think-block" :open="message.isThinking || message.waitingForFirstToken">
                  <summary class="think-summary">{{ message.isThinking || message.waitingForFirstToken ? t('assistant.thinkingActive') : t('assistant.thinking') }}</summary>
                  <div class="think-content">{{ message.thinkText || t('assistant.preparing') }}<span v-if="message.isThinking || message.waitingForFirstToken" class="think-cursor">▌</span></div>
                </details>
                <div v-html="renderAssistantMarkdown(message)" @click="onMarkdownClick"></div>
                <p v-if="message.error" class="bubble-error">{{ message.error }}</p>
                <div v-if="Array.isArray(message.sources)" class="sources-row">
                  <span class="sources-label">{{ t('assistant.sources') }}</span>
                  <template v-if="message.sources.length">
                    <template v-for="source in message.sources" :key="source.n">
                      <router-link
                        v-if="source.link"
                        class="source-chip"
                        :to="source.link"
                        :title="`${source.title} — ${source.source}`"
                      >
                        <span class="source-num">[{{ source.n }}]</span> {{ source.title }}
                        <span class="source-file">{{ source.source }}</span>
                      </router-link>
                      <span v-else class="source-chip" :title="`${source.title} — ${source.source}`">
                        <span class="source-num">[{{ source.n }}]</span> {{ source.title }}
                        <span class="source-file">{{ source.source }}</span>
                      </span>
                    </template>
                  </template>
                  <span v-else class="sources-empty">{{ t('assistant.noSources') }}</span>
                </div>
              </div>
              <div v-else class="bubble-content plain-content">{{ message.text }}</div>
            </div>
            <p v-if="!messages.length" class="empty-state">{{ t('assistant.startHint') }}</p>
          </div>

          <div class="controls">
            <textarea
              v-model="prompt"
              class="prompt-input"
              rows="4"
              :placeholder="t('assistant.promptPlaceholder')"
              @keydown.enter.ctrl.prevent="sendPrompt"
            ></textarea>

            <div class="preset-row">
              <span class="preset-label">{{ t('assistant.mode') }}</span>
              <div class="preset-group">
                <button
                  v-for="p in presets"
                  :key="p.id"
                  class="preset-btn"
                  :class="{ active: selectedPreset === p.id }"
                  :title="t(`assistant.presets.${p.id}.description`)"
                  @click="selectedPreset = p.id"
                >{{ t(`assistant.presets.${p.id}.label`) }}</button>
              </div>
              <label
                class="library-toggle"
                :title="t('assistant.useLibraryHint')"
              >
                <input v-model="useLibrary" type="checkbox" :disabled="streaming" />
                {{ t('assistant.useLibrary') }}
              </label>
            </div>

            <div class="action-row">
              <template v-if="!streaming">
                <button class="btn btn-primary" @click="sendPrompt" :disabled="!canSend">{{ t('assistant.send') }}</button>
              </template>
              <template v-else>
                <button class="btn btn-danger" @click="stopGeneration">{{ t('assistant.stop') }}</button>
              </template>
              <button class="btn btn-secondary" @click="restartConversation" :disabled="!messages.length && !prompt.trim()">{{ t('assistant.restart') }}</button>
              <button class="btn btn-secondary" @click="regenerate" :disabled="!messages.length">{{ t('assistant.regenerate') }}</button>
            </div>
          </div>
        </template>

        <div v-else class="assistant-gate">
          <div class="assistant-gate-card">
            <h3>{{ assistantEmptyTitle }}</h3>
            <p class="assistant-gate-body">{{ assistantEmptyBody }}</p>
            <p class="assistant-gate-note">{{ assistantGateNote }}</p>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>

<script setup>
import { computed, ref, onMounted, onBeforeUnmount, nextTick, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { formatBytes } from '../i18n/index.js'
import { apiService } from '../services/api'
import { useAuthState, isAdminLocked } from '../services/auth'
import { linkCitations, normalizeSources } from '../services/citations'
import { marked } from 'marked'
import DOMPurify from 'dompurify'

const { t } = useI18n()
const authState = useAuthState()
const adminLocked = computed(() => isAdminLocked())
const router = useRouter()

const STORAGE_KEY_MODEL = 'fyr_assistant_default_model'
const STORAGE_KEY_HISTORY = 'fyr_assistant_history'
const STORAGE_KEY_USE_LIBRARY = 'fyr_assistant_use_library'
const STORAGE_KEY_PRESET = 'fyr_assistant_preset'
const MAX_HISTORY_MESSAGES = 6
const LIBRARY_SOURCES_LIMIT = 4

const readPref = (key) => {
  try { return localStorage.getItem(key) } catch { return null }
}

const writePref = (key, value) => {
  try { localStorage.setItem(key, value) } catch { /* ignore storage failures */ }
}

const presets = [
  // Labels and descriptions: assistant.presets.<id>.label / .description
  { id: 'precise', temperature: 0.1, maxTokens: 512 },
  { id: 'balanced', temperature: 0.2, maxTokens: 512 },
  { id: 'creative', temperature: 0.7, maxTokens: 1024 }
]

const sidebarCollapsed = ref(false)
const models = ref([])
const selectedModel = ref(null)
const modelHealth = ref(null)
const messages = ref([])
const prompt = ref('')
const selectedPreset = ref(presets.some(p => p.id === readPref(STORAGE_KEY_PRESET)) ? readPref(STORAGE_KEY_PRESET) : 'balanced')
const useLibrary = ref(readPref(STORAGE_KEY_USE_LIBRARY) === 'true')
const loadingModel = ref(false)
const streaming = ref(false)
const chatHistoryRef = ref(null)
const activeAssistantMessage = ref(null)
let eventSource = null
let modelLoadSequence = 0

const currentPreset = computed(() => presets.find(p => p.id === selectedPreset.value) ?? presets[1])

const canSend = computed(() => {
  return !!selectedModel.value && !!modelHealth.value?.loaded && prompt.value.trim().length > 0 && !streaming.value && !loadingModel.value
})

const hasLoadedModel = computed(() => !!selectedModel.value && !!modelHealth.value?.loaded && !loadingModel.value)

const modelStatusText = computed(() => {
  if (!selectedModel.value) return t('assistant.status.none')
  if (loadingModel.value) return t('assistant.status.loading', { name: selectedModel.value.filename })
  if (!modelHealth.value) return t('assistant.status.selected', { name: selectedModel.value.filename })
  if (modelHealth.value.error) return t('assistant.status.health', { error: modelHealth.value.error })
  if (modelHealth.value.loaded) return t('assistant.status.loaded', { name: selectedModel.value.filename })
  return t('assistant.status.selected', { name: selectedModel.value.filename })
})

const assistantEmptyTitle = computed(() => {
  if (loadingModel.value) return t('assistant.gate.loadingTitle')
  if (!selectedModel.value) return t('assistant.gate.selectTitle')
  if (modelHealth.value?.error) return t('assistant.gate.unavailableTitle')
  return t('assistant.gate.notLoadedTitle')
})

const assistantEmptyBody = computed(() => {
  if (loadingModel.value) return modelStatusText.value
  if (!selectedModel.value) return t('assistant.gate.selectBody')
  if (modelHealth.value?.error) return modelStatusText.value
  return modelStatusText.value
})

const assistantGateNote = computed(() => {
  if (loadingModel.value) return t('assistant.gate.loadingNote')
  if (!selectedModel.value) return t('assistant.gate.selectNote')
  if (modelHealth.value?.error) return t('assistant.gate.unavailableNote')
  return t('assistant.gate.notLoadedNote')
})

const generateId = () => {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID()
  }
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = Math.random() * 16 | 0
    const v = c === 'x' ? r : (r & 0x3 | 0x8)
    return v.toString(16)
  })
}

const toggleSidebar = () => {
  sidebarCollapsed.value = !sidebarCollapsed.value
}

const scrollChatHistoryToBottom = async () => {
  await nextTick()
  const element = chatHistoryRef.value
  if (element) {
    element.scrollTop = element.scrollHeight
  }
}

const persistConversation = () => {
  const storedMessages = messages.value
    .filter((message) => !message.streaming)
    .map((message) => ({
      id: message.id,
      role: message.role,
      text: message.text || '',
      thinkText: message.thinkText || '',
      isThinking: false,
      streaming: false,
      ...(Array.isArray(message.sources) ? { sources: message.sources } : {}),
      ...(message.error ? { error: message.error } : {})
    }))

  try {
    localStorage.setItem(STORAGE_KEY_HISTORY, JSON.stringify({ messages: storedMessages }))
  } catch {
    // Ignore storage failures; the assistant still functions without persistence.
  }
}

const restoreConversation = () => {
  let stored = null
  try {
    stored = localStorage.getItem(STORAGE_KEY_HISTORY)
  } catch {
    stored = null
  }

  if (!stored) return

  try {
    const parsed = JSON.parse(stored)
    const restoredMessages = Array.isArray(parsed?.messages) ? parsed.messages : []
    messages.value = restoredMessages
      .filter((message) => message && typeof message === 'object' && typeof message.role === 'string')
      .map((message) => ({
        id: typeof message.id === 'string' && message.id ? message.id : generateId(),
        role: message.role,
        text: String(message.text || ''),
        thinkText: String(message.thinkText || ''),
        // A reload can never resume a stream, so never restore a live
        // "Thinking…" indicator.
        isThinking: false,
        streaming: false,
        sources: Array.isArray(message.sources) ? normalizeSources(message.sources) : null,
        error: typeof message.error === 'string' ? message.error : ''
      }))
  } catch {
    messages.value = []
  }
}

const activateModel = async (model) => {
  if (!model) return

  if (
    selectedModel.value?.filename === model.filename &&
    modelHealth.value?.loaded &&
    !loadingModel.value
  ) {
    return
  }

  const isModelSwitch = !!selectedModel.value?.filename && selectedModel.value.filename !== model.filename

  const selectionSequence = ++modelLoadSequence
  stopGeneration()

  if (isModelSwitch) {
    messages.value = []
    prompt.value = ''
    persistConversation()
  }

  selectedModel.value = model
  modelHealth.value = null
  loadingModel.value = true

  try {
    localStorage.setItem(STORAGE_KEY_MODEL, model.filename)
  } catch {
    // Ignore storage failures; the assistant still functions without persistence.
  }

  try {
    await loadHealth(model.filename)
    if (selectionSequence !== modelLoadSequence) return

    if (!modelHealth.value?.loaded) {
      await apiService.loadModel(model.filename)
      if (selectionSequence !== modelLoadSequence) return
      await loadHealth(model.filename)
    }
  } catch (error) {
    if (selectionSequence !== modelLoadSequence) return
    const detail = apiService.handleError(error)
    modelHealth.value = {
      loaded: false,
      error: detail
    }
  } finally {
    if (selectionSequence === modelLoadSequence) {
      loadingModel.value = false
    }
  }
}

const selectModel = async (model) => {
  await activateModel(model)
}

const buildHistory = () => {
  const completedMessages = messages.value.filter(
    m => !m.streaming && (m.role === 'user' || (m.role === 'assistant' && m.text?.trim()))
  )
  const recent = completedMessages.slice(-MAX_HISTORY_MESSAGES)
  return recent.map(m => ({ role: m.role, text: m.text }))
}

const sendPrompt = () => {
  if (!canSend.value) return

  stopGeneration()

  const userPrompt = prompt.value
  const history = buildHistory()
  messages.value.push({ id: generateId(), role: 'user', text: userPrompt })

  messages.value.push({
    id: generateId(),
    role: 'assistant',
    rawText: '',
    text: '',
    thinkText: '',
    isThinking: false,
    waitingForFirstToken: true,
    streaming: true,
    sources: null,
    error: ''
  })
  // Mutate through the reactive proxy (not the raw object) so streamed
  // tokens re-render immediately.
  const assistantMessage = messages.value[messages.value.length - 1]
  activeAssistantMessage.value = assistantMessage

  const preset = currentPreset.value
  streaming.value = true
  // Save the user's prompt now; the streaming reply is persisted on completion.
  persistConversation()
  eventSource = apiService.streamInference(
    selectedModel.value.filename,
    {
      prompt: userPrompt,
      temperature: preset.temperature,
      maxTokens: preset.maxTokens,
      history,
      useLibrary: useLibrary.value,
      sourcesLimit: LIBRARY_SOURCES_LIMIT
    },
    {
      onSources: (sources) => {
        assistantMessage.sources = sources
      },
      onToken: (token) => {
        assistantMessage.waitingForFirstToken = false
        assistantMessage.rawText += token
        const parsed = parseThinkAndText(assistantMessage.rawText)
        assistantMessage.text = parsed.text
        assistantMessage.thinkText = parsed.thinkText
        assistantMessage.isThinking = parsed.isThinking
        scrollChatHistoryToBottom()
      },
      onDone: () => {
        assistantMessage.streaming = false
        assistantMessage.isThinking = false
        assistantMessage.waitingForFirstToken = false
        activeAssistantMessage.value = null
        streaming.value = false
        eventSource = null
        scrollChatHistoryToBottom()
      },
      onError: (error) => {
        assistantMessage.error = describeStreamError(error)
        assistantMessage.streaming = false
        assistantMessage.isThinking = false
        assistantMessage.waitingForFirstToken = false
        activeAssistantMessage.value = null
        streaming.value = false
        eventSource = null
        scrollChatHistoryToBottom()
      }
    }
  )

  prompt.value = ''
  scrollChatHistoryToBottom()
}

const describeStreamError = (error) => {
  const status = error?.response?.status
  if (status === 409) {
    return t('assistant.errors.unloaded')
  }
  return t('assistant.errors.generation', { error: apiService.handleError(error) })
}

// Replace the last exchange instead of appending a duplicate prompt (which
// also fed the previous answer back to the model as history).
const regenerate = () => {
  if (streaming.value || loadingModel.value) return
  let lastUserIndex = -1
  for (let i = messages.value.length - 1; i >= 0; i -= 1) {
    if (messages.value[i].role === 'user') {
      lastUserIndex = i
      break
    }
  }
  if (lastUserIndex === -1) return
  const lastPrompt = messages.value[lastUserIndex].text
  messages.value.splice(lastUserIndex)
  prompt.value = lastPrompt
  sendPrompt()
}

const restartConversation = () => {
  stopGeneration()
  messages.value = []
  prompt.value = ''
  persistConversation()
}

const stopGeneration = () => {
  if (eventSource) {
    eventSource.close()
    eventSource = null
  }
  if (activeAssistantMessage.value) {
    activeAssistantMessage.value.streaming = false
    activeAssistantMessage.value.isThinking = false
    activeAssistantMessage.value.waitingForFirstToken = false
    activeAssistantMessage.value = null
  }
  streaming.value = false
}

const shouldShowThinkingBlock = (message) => {
  return message.role === 'assistant' && (
    !!message.thinkText
    || !!message.isThinking
    || !!message.waitingForFirstToken
  )
}

const renderMarkdown = (text) => {
  const rendered = marked.parse(text || '')
  return DOMPurify.sanitize(rendered, {
    USE_PROFILES: { html: true }
  })
}

// Assistant answers: link [n] citations to their library sources, then render.
const renderAssistantMarkdown = (message) => {
  const sources = Array.isArray(message.sources) ? message.sources : []
  return renderMarkdown(linkCitations(message.text, sources))
}

// Citation links live in v-html, so route them through the SPA router
// instead of a full page load (modifier clicks keep browser behaviour).
const onMarkdownClick = (event) => {
  const anchor = event.target?.closest?.('a.citation')
  if (!anchor) return
  if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return
  const href = anchor.getAttribute('href') || ''
  if (!href.startsWith('/') || href.startsWith('//')) return
  event.preventDefault()
  router.push(href)
}

// Split raw streamed text into visible response text and think-block content.
// Multiple <think>…</think> blocks are accumulated into a single thinkText.
// An unclosed <think> block sets isThinking=true so the UI can show a live indicator.
const parseThinkAndText = (raw) => {
  let text = ''
  let thinkText = ''
  let isThinking = false
  let remaining = raw

  while (remaining.length > 0) {
    if (!isThinking) {
      const thinkStart = remaining.indexOf('<think>')
      if (thinkStart === -1) {
        text += remaining
        break
      }
      text += remaining.slice(0, thinkStart)
      remaining = remaining.slice(thinkStart + '<think>'.length)
      isThinking = true
    } else {
      const thinkEnd = remaining.indexOf('</think>')
      if (thinkEnd === -1) {
        thinkText += remaining
        break
      }
      thinkText += remaining.slice(0, thinkEnd)
      remaining = remaining.slice(thinkEnd + '</think>'.length)
      isThinking = false
    }
  }

  return { text, thinkText, isThinking }
}

const loadModels = async () => {
  try {
    const response = await apiService.listAiModels()
    models.value = response.data || []
  } catch (error) {
    console.error('Failed to load models:', error)
  }
}

const loadHealth = async (filename) => {
  try {
    const response = await apiService.getModelHealth(filename)
    modelHealth.value = response.data || null
  } catch {
    modelHealth.value = null
  }
}

const restoreDefaultModel = async () => {
  let savedName = null
  try { savedName = localStorage.getItem(STORAGE_KEY_MODEL) } catch { /* ignore */ }
  if (!savedName) return

  const match = models.value.find(m => m.filename === savedName)
  if (!match) return

  await activateModel(match)
}

onMounted(async () => {
  restoreConversation()
  await loadModels()
  await restoreDefaultModel()
})

onBeforeUnmount(() => {
  stopGeneration()
  persistConversation()
})

// Persisting serializes the whole conversation; skip it for every streamed
// token and save once the stream settles (done/error/stop all flip `streaming`).
watch(messages, () => {
  if (!streaming.value) persistConversation()
}, { deep: true })
watch(streaming, (isStreaming) => {
  if (!isStreaming) persistConversation()
})
watch(useLibrary, (value) => writePref(STORAGE_KEY_USE_LIBRARY, value ? 'true' : 'false'))
watch(selectedPreset, (value) => writePref(STORAGE_KEY_PRESET, value))
</script>

<style scoped>
.assistant-page {
  height: 100%;
}

.assistant-layout {
  display: grid;
  grid-template-columns: 320px 1fr;
  gap: 1rem;
  min-height: calc(100vh - 230px);
}

.model-sidebar,
.chat-panel {
  background: #2a2a2a;
  border: 1px solid #3a3a3a;
  border-radius: 10px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
}

.model-sidebar {
  display: flex;
  flex-direction: column;
  transition: width 0.2s ease;
}

.model-sidebar.collapsed {
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
}

.icon-btn {
  background: #1a1a1a;
  color: #d9d9d9;
  border: 1px solid #4a4a4a;
  border-radius: 6px;
  width: 32px;
  height: 32px;
  cursor: pointer;
}

.sidebar-content {
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.hint-text {
  margin: 0;
  color: #b0b0b0;
  font-size: 0.82rem;
}

.import-link {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  text-decoration: none;
}

.model-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  max-height: calc(100vh - 350px);
  overflow: auto;
}

.model-item {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  padding: 0.65rem;
  border-radius: 6px;
  border: 1px solid #3f3f3f;
  background: #1a1a1a;
  color: #e0e0e0;
  cursor: pointer;
}

.model-item.active {
  border-color: #77b255;
  background: #253025;
}

.model-name {
  font-weight: 600;
  font-size: 0.9rem;
  word-break: break-all;
}

.model-meta {
  color: #b0b0b0;
  font-size: 0.8rem;
}

.chat-panel {
  display: flex;
  flex-direction: column;
  padding: 1rem;
  gap: 1rem;
}

.assistant-gate {
  flex: 1;
  min-height: 280px;
  display: grid;
  place-items: center;
}

.assistant-gate-card {
  width: min(620px, 100%);
  border: 1px solid #3a3a3a;
  border-radius: 12px;
  background: linear-gradient(180deg, #262626 0%, #1a1a1a 100%);
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.28);
  padding: 1.25rem 1.3rem;
}

.assistant-gate-card h3 {
  margin: 0;
  color: #f2f2f2;
  font-size: 1.25rem;
}

.assistant-gate-body {
  margin: 0.5rem 0 0;
  color: #c9c9c9;
  line-height: 1.55;
}

.assistant-gate-note {
  margin: 0.75rem 0 0;
  color: #a9a9a9;
  font-size: 0.9rem;
}

.status-row {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
}

.status-model {
  margin: 0;
  color: #d0d0d0;
}

.status-model.loaded {
  color: #8fd28f;
}

.chat-history {
  flex: 1;
  min-height: 280px;
  max-height: calc(100vh - 420px);
  overflow: auto;
  background: #1a1a1a;
  border: 1px solid #383838;
  border-radius: 8px;
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.bubble {
  max-width: 78%;
  border-radius: 10px;
  padding: 0.65rem 0.8rem;
  line-height: 1.4;
}

.bubble-content {
  overflow-wrap: anywhere;
}

.plain-content {
  white-space: pre-wrap;
}

.markdown-content :deep(p),
.markdown-content :deep(ul),
.markdown-content :deep(ol),
.markdown-content :deep(blockquote),
.markdown-content :deep(pre) {
  margin: 0 0 0.75rem;
}

.markdown-content :deep(p:last-child),
.markdown-content :deep(ul:last-child),
.markdown-content :deep(ol:last-child),
.markdown-content :deep(blockquote:last-child),
.markdown-content :deep(pre:last-child) {
  margin-bottom: 0;
}

.markdown-content :deep(h1),
.markdown-content :deep(h2),
.markdown-content :deep(h3),
.markdown-content :deep(h4) {
  margin: 0 0 0.6rem;
  line-height: 1.2;
}

.markdown-content :deep(code) {
  font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
  font-size: 0.95em;
}

.markdown-content :deep(pre) {
  overflow: auto;
  padding: 0.75rem;
  border-radius: 8px;
  background: rgba(0, 0, 0, 0.25);
}

.markdown-content :deep(pre code) {
  white-space: pre;
}

.markdown-content :deep(a) {
  color: #9ec3ff;
}

.markdown-content :deep(blockquote) {
  padding-left: 0.75rem;
  border-left: 3px solid rgba(255, 255, 255, 0.25);
  color: #d0d8d0;
}

.bubble.user {
  align-self: flex-end;
  background: #36445d;
}

.bubble.assistant {
  align-self: flex-start;
  background: #304031;
}

.controls {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.prompt-input {
  width: 100%;
  background: #151515;
  color: #e0e0e0;
  border: 1px solid #3a3a3a;
  border-radius: 8px;
  padding: 0.75rem;
  resize: vertical;
}

.preset-row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.preset-label {
  color: #b0b0b0;
  font-size: 0.85rem;
  white-space: nowrap;
}

.preset-group {
  display: flex;
  gap: 0.4rem;
}

.preset-btn {
  background: #1a1a1a;
  color: #c0c0c0;
  border: 1px solid #4a4a4a;
  border-radius: 6px;
  padding: 0.3rem 0.75rem;
  font-size: 0.85rem;
  cursor: pointer;
}

.preset-btn.active {
  background: #253025;
  border-color: #77b255;
  color: #8fd28f;
  font-weight: 600;
}

.library-toggle {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  margin-left: auto;
  color: #c0c0c0;
  font-size: 0.85rem;
  cursor: pointer;
  user-select: none;
}

.bubble-error {
  margin: 0.5rem 0 0;
  color: #ffb4a8;
  font-size: 0.85rem;
}

.sources-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.35rem;
  margin-top: 0.6rem;
  padding-top: 0.5rem;
  border-top: 1px solid rgba(255, 255, 255, 0.12);
}

.sources-label {
  color: #a9c4a9;
  font-size: 0.78rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  margin-right: 0.2rem;
}

.sources-empty {
  color: #a0a0a0;
  font-size: 0.82rem;
  font-style: italic;
}

.source-chip {
  display: inline-flex;
  align-items: baseline;
  gap: 0.3rem;
  max-width: 100%;
  padding: 0.2rem 0.55rem;
  border: 1px solid #4a5f4a;
  border-radius: 999px;
  background: #1f2a1f;
  color: #dfe9df;
  font-size: 0.8rem;
  text-decoration: none;
  overflow-wrap: anywhere;
}

a.source-chip:hover {
  border-color: #77b255;
  background: #253025;
}

.source-num {
  color: #8fd28f;
  font-weight: 600;
}

.source-file {
  color: #9aa89a;
  font-size: 0.75rem;
}

.markdown-content :deep(.citation) {
  color: #8fd28f;
  font-size: 0.85em;
  font-weight: 600;
  text-decoration: none;
  vertical-align: super;
  line-height: 0;
}

.markdown-content :deep(a.citation:hover) {
  text-decoration: underline;
}

.action-row {
  display: flex;
  flex-wrap: wrap;
  gap: 0.6rem;
}

.btn {
  border: 0;
  border-radius: 8px;
  padding: 0.55rem 0.9rem;
  font-weight: 600;
  cursor: pointer;
}

.btn-primary {
  background: #6291ff;
  color: #fff;
}

.btn-secondary {
  background: #424242;
  color: #f1f1f1;
}

.btn-danger {
  background: #914848;
  color: #fff;
}

.empty-state {
  color: #9a9a9a;
  font-style: italic;
  margin: 0;
}

.think-block {
  margin-bottom: 0.6rem;
  border: 1px solid #3a4a3a;
  border-radius: 6px;
  overflow: hidden;
}

.think-summary {
  padding: 0.35rem 0.6rem;
  font-size: 0.82rem;
  color: #8fb08f;
  cursor: pointer;
  user-select: none;
  background: #1e2e1e;
  list-style: none;
}

.think-summary::-webkit-details-marker {
  display: none;
}

.think-summary::before {
  content: '▶ ';
  font-size: 0.7em;
}

details.think-block[open] .think-summary::before {
  content: '▼ ';
}

.think-content {
  padding: 0.5rem 0.75rem;
  font-size: 0.82rem;
  color: #8a9e8a;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  max-height: 12rem;
  overflow-y: auto;
  background: #171f17;
}

.think-cursor {
  display: inline-block;
  animation: blink 1s step-end infinite;
}

@keyframes blink {
  50% { opacity: 0; }
}

@media (max-width: 1024px) {
  .assistant-layout {
    grid-template-columns: 1fr;
  }

  .model-sidebar.collapsed {
    width: 100%;
  }
}
</style>
