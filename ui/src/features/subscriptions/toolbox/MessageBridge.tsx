import { useEffect } from 'react'
import { useToast } from '@acme/components'
import { TOOLBOX_MESSAGE_EVENT, type MessageTone } from '@/lib/message'

export function MessageBridge() {
  const toast = useToast()
  useEffect(() => {
    const listener = (event: Event) => {
      const detail = (event as CustomEvent<{ tone: MessageTone; content: string }>).detail
      toast[detail.tone](detail.content)
    }
    window.addEventListener(TOOLBOX_MESSAGE_EVENT, listener)
    return () => window.removeEventListener(TOOLBOX_MESSAGE_EVENT, listener)
  }, [toast])
  return null
}
