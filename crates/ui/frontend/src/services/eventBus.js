const listeners = new Map()

export const onAppEvent = (eventName, handler) => {
  if (!listeners.has(eventName)) {
    listeners.set(eventName, new Set())
  }

  const handlers = listeners.get(eventName)
  handlers.add(handler)

  return () => {
    handlers.delete(handler)
    if (handlers.size === 0) {
      listeners.delete(eventName)
    }
  }
}

export const emitAppEvent = (eventName, payload) => {
  const handlers = listeners.get(eventName)
  if (!handlers) return

  handlers.forEach((handler) => handler(payload))
}