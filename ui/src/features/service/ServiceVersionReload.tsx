import { useEffect, useRef } from 'react'
import { useQueryClient, type QueryClient } from '@tanstack/react-query'
import { readServiceUpdateMarker, serviceUpdateTaskKey } from '../../lib/useServiceUpdateTask'
import type { ServiceUpdateTask } from '../../lib/types'

const POLL_INTERVAL_MS = 5000

export function ServiceVersionReload() {
  const client = useQueryClient()
  useServiceVersionChange((version) => {
    if (!completeServiceUpdateOnVersionChange(client, version)) window.location.reload()
  })
  return null
}

export function useServiceVersionChange(onChange: (version: string) => void) {
  const onChangeRef = useRef(onChange)
  useEffect(() => {
    onChangeRef.current = onChange
  }, [onChange])

  useEffect(() => {
    const controller = new AbortController()
    let baseline = ''
    let timer: ReturnType<typeof setTimeout> | undefined

    const poll = async () => {
      try {
        const response = await fetch(`${window.location.origin}/api/v1/health`, {
          cache: 'no-store',
          signal: controller.signal,
        })
        if (response.ok) {
          const health = await response.json() as { version?: string }
          const version = health.version?.trim() || ''
          if (version && baseline && version !== baseline) {
            onChangeRef.current(version)
            return
          }
          if (version) baseline = version
        }
      } catch {
        // The service is expected to be briefly unavailable while it upgrades.
      }
      if (!controller.signal.aborted) timer = setTimeout(poll, POLL_INTERVAL_MS)
    }

    void poll()
    return () => {
      controller.abort()
      if (timer) clearTimeout(timer)
    }
  }, [])
}

function normalizeVersion(version: string) {
  return version.replace(/^v/, '')
}

export function completeServiceUpdateOnVersionChange(client: QueryClient, version: string) {
  const marker = readServiceUpdateMarker()
  if (!marker || normalizeVersion(marker.targetVersion) !== normalizeVersion(version)) return false
  client.setQueryData<{ task: ServiceUpdateTask | null }>(serviceUpdateTaskKey, (current) => {
    if (!current?.task) return current
    const now = new Date().toISOString()
    return { task: { ...current.task, state: 'succeeded', stage: 'completed', target_version: version, updated_at: now, finished_at: now, eta_seconds: undefined, error: undefined } }
  })
  return true
}
