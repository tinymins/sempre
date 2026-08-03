import { useEffect } from 'react'
import { streamEvents } from './api'
import { useSession } from './session'
import type { RuntimeEvent } from './types'

export function useRuntimeEvents(topics: string[], onEvent: (event: RuntimeEvent) => void, enabled = true) {
  const { session } = useSession()
  const topicKey = topics.join(',')
  useEffect(() => {
    if (!session || !enabled) return
    const selectedTopics = topicKey.split(',').filter(Boolean)
    const controller = new AbortController()
    let retry: number | undefined
    const connect = () => {
      streamEvents(session, selectedTopics, onEvent, controller.signal).catch(() => {
        if (!controller.signal.aborted) retry = window.setTimeout(connect, 1500)
      })
    }
    connect()
    return () => {
      controller.abort()
      if (retry) window.clearTimeout(retry)
    }
  }, [session, topicKey, enabled, onEvent])
}
