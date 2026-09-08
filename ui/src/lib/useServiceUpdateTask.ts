import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from './api'
import { useSession } from './session'
import type { ServiceUpdateTask } from './types'
import { clearServiceUpdateMarker, readServiceUpdateMarker, writeServiceUpdateMarker } from './serviceUpdateState'

export const serviceUpdateTaskKey = ['service', 'update-task']
export { clearServiceUpdateMarker, readServiceUpdateMarker, writeServiceUpdateMarker } from './serviceUpdateState'

export function useServiceUpdateTask() {
  const { session } = useSession()
  const client = useQueryClient()
  const marker = readServiceUpdateMarker()
  const query = useQuery({
    queryKey: serviceUpdateTaskKey,
    queryFn: async ({ signal }) => {
      const current = readServiceUpdateMarker()
      const result = await api<{ task: ServiceUpdateTask | null }>(session!, '/service/update/task', { signal })
      if (current && result.task) writeServiceUpdateMarker({ ...current, task: result.task })
      return current?.task && !result.task ? { task: current.task } : result
    },
    enabled: (query) => Boolean(session) && !(marker && ['succeeded', 'failed'].includes(query.state.data?.task?.state || '')),
    retry: false,
    refetchInterval: (query) => query.state.data?.task?.state === 'running' ? 500 : 3000,
    refetchIntervalInBackground: true,
  })
  const mutation = useMutation({
    mutationKey: serviceUpdateTaskKey,
    mutationFn: (targetVersion: string) => {
      writeServiceUpdateMarker({ targetVersion, baseURL: session!.baseURL })
      return api<{ task: ServiceUpdateTask }>(session!, '/service/update', { method: 'POST' })
    },
    onSuccess: (result) => {
      writeServiceUpdateMarker({ targetVersion: result.task.target_version || readServiceUpdateMarker()!.targetVersion, baseURL: session!.baseURL, task: result.task })
      client.setQueryData(serviceUpdateTaskKey, result)
    },
    onError: () => clearServiceUpdateMarker(),
    onSettled: () => { void client.invalidateQueries({ queryKey: serviceUpdateTaskKey }) },
  })
  return { task: query.data?.task, query, mutation }
}
