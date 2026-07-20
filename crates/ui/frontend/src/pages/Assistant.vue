<template>
  <div class="assistant-page">
    <div class="assistant-layout">
      <aside class="model-sidebar" :class="{ collapsed: sidebarCollapsed }">
        <div class="sidebar-header">
          <h3>Model Library</h3>
          <button class="icon-btn" @click="toggleSidebar" :title="sidebarCollapsed ? 'Expand panel' : 'Collapse panel'">
            {{ sidebarCollapsed ? '»' : '«' }}
          </button>
        </div>

        <div v-if="!sidebarCollapsed" class="sidebar-content">
          <router-link
            v-if="!adminLocked"
            class="btn btn-primary import-link"
            :to="{ name: 'ContentManager', query: { category: 'models' } }"
          >
            Open Content Manager
          </router-link>
          <router-link
            v-else-if="authState.requiresAuth && !authState.readonly"
            class="btn btn-secondary import-link"
            to="/login"
          >
            Log in to manage models
          </router-link>
          <p class="hint-text">Upload `.gguf` models from Content Manager. Fyr stages them in /data/inbox and routes them into /data/models.</p>

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
            <p v-if="!models.length" class="empty-state">No GGUF models found in /data/models.</p>
          </div>
        </div>
      </aside>

      <section class="chat-panel">
        <div class="status-row">
          <p class="status-label">
            Offline Mode: Active
          </p>
          <p class="status-model" :class="{ loaded: !!selectedModel }">
            {{ modelStatusText }}
          </p>
        </div>

        <div ref="chatHistoryRef" class="chat-history">
          <div v-for="message in messages" :key="message.id" class="bubble" :class="[message.role, { streaming: message.streaming }]">
            <div
              v-if="message.role === 'assistant'"
              class="bubble-content markdown-content"
            >
              <details v-if="message.thinkText || message.isThinking" class="think-block" :open="message.isThinking">
                <summary class="think-summary">{{ message.isThinking ? 'Thinking…' : 'Thinking' }}</summary>
                <div class="think-content">{{ message.thinkText }}<span v-if="message.isThinking" class="think-cursor">▌</span></div>
              </details>
              <div v-html="renderMarkdown(message.text)"></div>
              <p v-if="message.streaming" class="streaming-indicator">Generating…</p>
            </div>
            <div v-else class="bubble-content plain-content">{{ message.text }}</div>
          </div>
          <p v-if="!messages.length" class="empty-state">Select a model and start chatting offline.</p>
        </div>

        <div class="controls">
          <textarea
            v-model="prompt"
            class="prompt-input"
            rows="4"
            placeholder="Write your prompt..."
          ></textarea>

          <div class="control-grid">
            <label>
              Temperature
              <input v-model.number="temperature" type="range" min="0" max="2" step="0.1" />
              <span>{{ temperature.toFixed(1) }}</span>
            </label>
            <label>
              Max Tokens
              <input v-model.number="maxTokens" type="range" min="32" max="2048" step="32" />
              <span>{{ maxTokens }}</span>
            </label>
          </div>

          <div class="action-row">
            <button class="btn btn-primary" @click="sendPrompt" :disabled="!canSend">Send</button>
            <button class="btn btn-secondary" @click="loadSelectedModel" :disabled="!selectedModel || loadingModel">
              {{ loadingModel ? 'Loading...' : 'Load Model' }}
            </button>
            <button class="btn btn-secondary" @click="regenerate" :disabled="!messages.length">Regenerate</button>
            <button class="btn btn-danger" @click="stopGeneration" :disabled="!streaming">Stop</button>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>

<script setup>
import { computed, ref, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { apiService } from '../services/api'
import { useAuthState, isAdminLocked } from '../services/auth'
import { marked } from 'marked'
import DOMPurify from 'dompurify'

const authState = useAuthState()
const adminLocked = computed(() => isAdminLocked())

const sidebarCollapsed = ref(false)
const models = ref([])
const selectedModel = ref(null)
const modelHealth = ref(null)
const messages = ref([])
const prompt = ref('')
const temperature = ref(0.7)
const maxTokens = ref(512)
const loadingModel = ref(false)
const streaming = ref(false)
const chatHistoryRef = ref(null)
const activeAssistantMessage = ref(null)
let eventSource = null

const canSend = computed(() => {
  return !!selectedModel.value && !!modelHealth.value?.loaded && prompt.value.trim().length > 0 && !streaming.value
})

const modelStatusText = computed(() => {
  if (!selectedModel.value) return 'No model selected'
  if (!modelHealth.value) return `Model selected: ${selectedModel.value.filename}`
  if (modelHealth.value.error) return `Health check: ${modelHealth.value.error}`
  if (modelHealth.value.loaded) return `Model loaded for validation: ${selectedModel.value.filename}`
  return `Model selected: ${selectedModel.value.filename}`
})

const formatBytes = (bytes) => {
  if (!bytes) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i]
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

const selectModel = (model) => {
  selectedModel.value = model
  loadHealth(model.filename)
}

const loadSelectedModel = async () => {
  if (!selectedModel.value) return
  loadingModel.value = true

  try {
    await apiService.loadModel(selectedModel.value.filename)
    await loadHealth(selectedModel.value.filename)
    const statusDetail = modelHealth.value?.loaded
      ? `Model loaded for inference: ${selectedModel.value.filename}`
      : `Model validated: ${selectedModel.value.filename}. ${modelHealth.value?.error || 'Inference is not available yet for this runtime.'}`
    messages.value.push({
      id: crypto.randomUUID(),
      role: 'assistant',
      text: statusDetail
    })
  } catch (error) {
    const detail = apiService.handleError(error)
    messages.value.push({
      id: crypto.randomUUID(),
      role: 'assistant',
      text: `Health check: could not load ${selectedModel.value.filename}. ${detail}`
    })
  } finally {
    loadingModel.value = false
  }
}

const sendPrompt = () => {
  if (!canSend.value) return

  stopGeneration()

  const userPrompt = prompt.value
  messages.value.push({ id: crypto.randomUUID(), role: 'user', text: userPrompt })

  const assistantMessage = {
    id: crypto.randomUUID(),
    role: 'assistant',
    rawText: '',
    text: '',
    thinkText: '',
    isThinking: false,
    streaming: true
  }
  messages.value.push(assistantMessage)
  activeAssistantMessage.value = assistantMessage

  streaming.value = true
  eventSource = apiService.streamInference(
    selectedModel.value.filename,
    {
      prompt: userPrompt,
      temperature: temperature.value,
      maxTokens: maxTokens.value
    },
    {
      onToken: (token) => {
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
        activeAssistantMessage.value = null
        streaming.value = false
        eventSource = null
        scrollChatHistoryToBottom()
      },
      onError: () => {
        assistantMessage.streaming = false
        assistantMessage.isThinking = false
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

const regenerate = () => {
  const lastUser = [...messages.value].reverse().find(msg => msg.role === 'user')
  if (!lastUser) return
  prompt.value = lastUser.text
  sendPrompt()
}

const stopGeneration = () => {
  if (eventSource) {
    eventSource.close()
    eventSource = null
  }
  if (activeAssistantMessage.value) {
    activeAssistantMessage.value.streaming = false
    activeAssistantMessage.value = null
  }
  streaming.value = false
}

const renderMarkdown = (text) => {
  const rendered = marked.parse(text || '')
  return DOMPurify.sanitize(rendered, {
    USE_PROFILES: { html: true }
  })
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

onMounted(async () => {
  await loadModels()
})

onBeforeUnmount(() => {
  stopGeneration()
})
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

.status-row {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
}

.status-label {
  margin: 0;
  color: #8fd28f;
  font-weight: 600;
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

.chat-history :deep(.streaming-indicator) {
  margin: 0.45rem 0 0;
  font-size: 0.82rem;
  color: #9ebf9f;
  opacity: 0.9;
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

.control-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 1rem;
}

.control-grid label {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  color: #c9c9c9;
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
