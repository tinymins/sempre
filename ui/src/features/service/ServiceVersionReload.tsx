import { useEffect, useRef } from 'react'
import { useQueryClient, type QueryClient } from '@tanstack/react-query'
import { readServiceUpdateMarker, serviceUpdateTaskKey } from '../../lib/useServiceUpdateTask'
import type { ServiceUpdateTask } from '../../lib/types'
import { useSession } from '../../lib/session'
import { writeServiceUpdateMarker } from '../../lib/serviceUpdateState'

interface UiIdentity { id: string; version: string }

const POLL_INTERVAL_MS = 5000

export function ServiceVersionReload() {
  const client = useQueryClient()
  const { session } = useSession()
  const baseURL = readServiceUpdateMarker()?.baseURL || session?.baseURL || window.location.origin
  useServiceVersionChange((version, ui) => {
    if (!completeServiceUpdateOnVersionChange(client, version, ui)) window.location.reload()
  }, baseURL)
  return null
}

export function useServiceVersionChange(onChange: (version: string, ui?: UiIdentity | null) => void, baseURL = window.location.origin) {
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
        const response = await fetch(`${baseURL}/api/v1/health`, {
          cache: 'no-store',
          signal: controller.signal,
        })
        if (response.ok) {
          const health = await response.json() as { version?: string; ui?: UiIdentity | null }
          const version = health.version?.trim() || ''
          const identity = `${version}|${health.ui?.id || ''}|${health.ui?.version || ''}`
          const marker = readServiceUpdateMarker()
          const changed = marker ? matchesTarget(marker.targetVersion, version, health.ui) : Boolean(baseline && identity !== baseline)
          if (version && changed) {
            onChangeRef.current(version, health.ui)
            return
          }
          if (version) baseline = identity
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
  }, [baseURL])
}

function normalizeVersion(version: string) {
  return version.replace(/^v/, '')
}

function matchesTarget(target: string, version: string, ui?: UiIdentity | null) {
  return normalizeVersion(target) === normalizeVersion(version)
    && (ui === undefined || Boolean(ui?.id && normalizeVersion(ui.version) === normalizeVersion(target)))
}

export function completeServiceUpdateOnVersionChange(client: QueryClient, version: string, ui?: UiIdentity | null) {
  const marker = readServiceUpdateMarker()
  if (!marker) return false
  if (!matchesTarget(marker.targetVersion, version, ui)) return true
  void client.cancelQueries({ queryKey: serviceUpdateTaskKey })
  client.setQueryData<{ task: ServiceUpdateTask | null }>(serviceUpdateTaskKey, (current) => {
    const task = current?.task || marker.task
    if (!task || task.state === 'failed') return current
    const now = new Date().toISOString()
    const completed: ServiceUpdateTask = { ...task, state: 'succeeded', stage: 'completed', target_version: version, updated_at: now, finished_at: now, eta_seconds: undefined, error: undefined }
    writeServiceUpdateMarker({ ...marker, task: completed })
    return { task: completed }
  })
  return true
}
