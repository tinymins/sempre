export type MessageTone = 'success' | 'error' | 'info' | 'warning'

export const TOOLBOX_MESSAGE_EVENT = 'sempre:toolbox-message'

function emit(tone: MessageTone, content: string) {
  window.dispatchEvent(new CustomEvent(TOOLBOX_MESSAGE_EVENT, { detail: { tone, content } }))
}

export const message = {
  success: (content: string) => emit('success', content),
  error: (content: string) => emit('error', content),
  info: (content: string) => emit('info', content),
  warning: (content: string) => emit('warning', content),
}
