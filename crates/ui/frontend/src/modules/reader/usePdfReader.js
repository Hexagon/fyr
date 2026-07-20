import { ref } from 'vue'

export const usePdfReader = () => {
  const url = ref('')

  const open = async (descriptor) => {
    url.value = descriptor.content_url || ''
  }

  const dispose = () => {
    url.value = ''
  }

  return {
    url,
    open,
    dispose
  }
}
