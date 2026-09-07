import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from './api'
import { useSession } from './session'
import type { ServiceUpdateTask } from './types'

export const serviceUpdateTaskKey = ['service', 'update-task']
export const serviceUpdateSessionKey = 'sempre.service-update.v1'

export interface ServiceUpdateMarker {
  targetVersion: string
}

export function useServiceUpdateTask() {
  const { session } = useSession()
  const client = useQueryClient()
  const query = useQuery({
    queryKey: serviceUpdateTaskKey,
    queryFn: () => api<{ task: ServiceUpdateTask | null }>(session!, '/service/update/task'),
    enabled: Boolean(session),
    retry: false,
    refetchInterval: (query) => query.state.data?.task?.state === 'running' ? 500 : 3000,
    refetchIntervalInBackground: true,
  })
  const mutation = useMutation({
    mutationKey: serviceUpdateTaskKey,
    mutationFn: (targetVersion: string) => {
      writeServiceUpdateMarker({ targetVersion })
      return api<{ task: ServiceUpdateTask }>(session!, '/service/update', { method: 'POST' })
    },
    onSuccess: (result) => client.setQueryData(serviceUpdateTaskKey, result),
    onError: () => clearServiceUpdateMarker(),
    onSettled: () => { void client.invalidateQueries({ queryKey: serviceUpdateTaskKey }) },
  })
  return { task: query.data?.task, query, mutation }
}

export function readServiceUpdateMarker(): ServiceUpdateMarker | null {
  try {
    const value = JSON.parse(sessionStorage.getItem(serviceUpdateSessionKey) || 'null') as ServiceUpdateMarker | null
    return value?.targetVersion ? value : null
  } catch {
    return null
  }
}

export function writeServiceUpdateMarker(marker: ServiceUpdateMarker) {
  sessionStorage.setItem(serviceUpdateSessionKey, JSON.stringify(marker))
}

export function clearServiceUpdateMarker() {
  sessionStorage.removeItem(serviceUpdateSessionKey)
}
