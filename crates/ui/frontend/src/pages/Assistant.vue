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
          <button class="btn btn-primary" @click="openImportDialog">
            Import Model
          </button>
          <input
            ref="modelFileInput"
            class="hidden-input"
            type="file"
            accept=".gguf"
            @change="onModelFilePicked"
          />

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

        <div class="chat-history">
          <div v-for="message in messages" :key="message.id" class="bubble" :class="message.role">
            {{ message.text }}
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
import { computed, ref, onMounted, onBeforeUnmount } from 'vue'
import { apiService } from '../services/api'

const sidebarCollapsed = ref(false)
const modelFileInput = ref(null)
const models = ref([])
const selectedModel = ref(null)
const modelHealth = ref(null)
const messages = ref([])
const prompt = ref('')
const temperature = ref(0.7)
const maxTokens = ref(512)
const loadingModel = ref(false)
const streaming = ref(false)
const modelImportSource = ref('inbox')
let eventSource = null

const canSend = computed(() => {
  return !!selectedModel.value && !!modelHealth.value?.loaded && prompt.value.trim().length > 0 && !streaming.value
})

const modelStatusText = computed(() => {
  if (!selectedModel.value) return 'No model selected'
  if (!modelHealth.value) return `Model selected: ${selectedModel.value.filename}`
  if (modelHealth.value.loaded) return `Model loaded: ${selectedModel.value.filename}`
  if (modelHealth.value.error) return `Health check: ${modelHealth.value.error}`
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

const openImportDialog = () => {
  modelFileInput.value?.click()
}

const onModelFilePicked = async (event) => {
  const file = event.target?.files?.[0]
  if (!file) return

  try {
    await apiService.importModel(file.name, modelImportSource.value)
    messages.value.push({
      id: crypto.randomUUID(),
      role: 'assistant',
      text: `Imported ${file.name} to /data/models.`
    })
    await loadModels()
  } catch (error) {
    const detail = apiService.handleError(error)
    messages.value.push({
      id: crypto.randomUUID(),
      role: 'assistant',
      text: `Health check: import failed for ${file.name}. ${detail}`
    })
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
    messages.value.push({
      id: crypto.randomUUID(),
      role: 'assistant',
      text: `Model loaded: ${selectedModel.value.filename}`
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
    text: ''
  }
  messages.value.push(assistantMessage)

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
        assistantMessage.text += token
      },
      onError: () => {
        streaming.value = false
      }
    }
  )

  prompt.value = ''
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
  streaming.value = false
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

.hidden-input {
  display: none;
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

.bubble {
  max-width: 78%;
  border-radius: 10px;
  padding: 0.65rem 0.8rem;
  line-height: 1.4;
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

@media (max-width: 1024px) {
  .assistant-layout {
    grid-template-columns: 1fr;
  }

  .model-sidebar.collapsed {
    width: 100%;
  }
}
</style>
