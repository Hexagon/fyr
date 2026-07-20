<template>
  <div class="login-page">
    <div class="login-card">
      <h2>Admin Login</h2>
      <p class="hint">This server requires a password to access the Content Manager and other admin features.</p>

      <form @submit.prevent="handleLogin" class="login-form">
        <label class="field-label" for="password">Admin Password</label>
        <input
          id="password"
          v-model="password"
          type="password"
          class="password-input"
          placeholder="Enter admin password"
          autocomplete="current-password"
          :disabled="loading"
        />

        <p v-if="error" class="error-text">{{ error }}</p>

        <button type="submit" class="btn btn-primary" :disabled="loading || !password">
          {{ loading ? 'Logging in…' : 'Log In' }}
        </button>
      </form>

      <p class="back-hint">
        <router-link to="/">← Back to Overview</router-link>
      </p>
    </div>
  </div>
</template>

<script setup>
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { login } from '../services/auth'

const router = useRouter()
const password = ref('')
const loading = ref(false)
const error = ref(null)

const handleLogin = async () => {
  if (!password.value || loading.value) return
  loading.value = true
  error.value = null
  try {
    await login(password.value)
    const redirect = router.currentRoute.value.query.redirect || '/content'
    router.push(redirect)
  } catch (err) {
    const status = err?.response?.status
    if (status === 429) {
      error.value = 'Too many failed attempts. Please wait a few minutes and try again.'
    } else if (status === 401) {
      error.value = 'Incorrect password. Please try again.'
    } else {
      error.value = err?.response?.data?.error || 'Login failed. Please try again.'
    }
  } finally {
    loading.value = false
    password.value = ''
  }
}
</script>

<style scoped>
.login-page {
  display: flex;
  justify-content: center;
  align-items: flex-start;
  padding-top: 6rem;
  min-height: 60vh;
}

.login-card {
  background: var(--card-bg, #262626);
  border: 1px solid var(--border-color, #3a3a3a);
  border-radius: 0.75rem;
  padding: 2.5rem 2rem;
  width: 100%;
  max-width: 22rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.login-card h2 {
  margin: 0;
  font-size: 1.35rem;
  color: var(--text-primary, #e0e0e0);
}

.hint {
  margin: 0;
  font-size: 0.875rem;
  color: var(--text-secondary, #b0b0b0);
  line-height: 1.4;
}

.login-form {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.field-label {
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--text-secondary, #b0b0b0);
}

.password-input {
  width: 100%;
  padding: 0.6rem 0.75rem;
  background: var(--bg-primary, #1a1a1a);
  border: 1px solid var(--border-color, #3a3a3a);
  border-radius: 0.4rem;
  color: var(--text-primary, #e0e0e0);
  font-size: 0.95rem;
  box-sizing: border-box;
}

.password-input:focus {
  outline: none;
  border-color: #667eea;
}

.error-text {
  margin: 0;
  font-size: 0.875rem;
  color: #f87171;
}

.btn {
  padding: 0.6rem 1.25rem;
  border: none;
  border-radius: 0.4rem;
  cursor: pointer;
  font-size: 0.95rem;
  font-weight: 600;
  transition: opacity 0.2s;
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-primary {
  background: #667eea;
  color: white;
}

.btn-primary:hover:not(:disabled) {
  background: #5a6fd6;
}

.back-hint {
  margin: 0;
  font-size: 0.85rem;
  color: var(--text-secondary, #b0b0b0);
  text-align: center;
}

.back-hint a {
  color: #667eea;
  text-decoration: none;
}

.back-hint a:hover {
  text-decoration: underline;
}
</style>
