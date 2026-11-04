import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, uploadServiceUpdate } from './api'
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
      if (current && result.task) writeServiceUpdateMarker({ ...current, targetVersion: result.task.target_version || current.targetVersion, task: result.task })
      return current?.task && !result.task ? { task: current.task } : result
    },
    enabled: (query) => Boolean(session) && !(marker && ['succeeded', 'failed', 'cancelled'].includes(query.state.data?.task?.state || '')),
    retry: false,
    refetchInterval: (query) => query.state.data?.task?.state === 'running' ? 500 : 3000,
    refetchIntervalInBackground: true,
  })
  const mutation = useMutation({
    mutationKey: serviceUpdateTaskKey,
    mutationFn: () => {
      writeServiceUpdateMarker({ targetVersion: '', baseURL: session!.baseURL })
      return api<{ task: ServiceUpdateTask }>(session!, '/service/update', { method: 'POST' })
    },
    onSuccess: (result) => {
      writeServiceUpdateMarker({ targetVersion: result.task.target_version || readServiceUpdateMarker()!.targetVersion, baseURL: session!.baseURL, task: result.task })
      client.setQueryData(serviceUpdateTaskKey, result)
    },
    onError: () => clearServiceUpdateMarker(),
    onSettled: () => { void client.invalidateQueries({ queryKey: serviceUpdateTaskKey }) },
  })
  const uploadMutation = useMutation({
    mutationKey: serviceUpdateTaskKey,
    mutationFn: ({ file }: { file: File }) => {
      writeServiceUpdateMarker({ targetVersion: '', baseURL: session!.baseURL })
      return uploadServiceUpdate(session!, file)
    },
    onSuccess: (result) => {
      writeServiceUpdateMarker({ targetVersion: result.task.target_version || readServiceUpdateMarker()!.targetVersion, baseURL: session!.baseURL, task: result.task })
      client.setQueryData(serviceUpdateTaskKey, result)
    },
    onError: () => clearServiceUpdateMarker(),
    onSettled: () => { void client.invalidateQueries({ queryKey: serviceUpdateTaskKey }) },
  })
  const confirmMutation = useMutation({
    mutationFn: ({ id, confirmed }: { id: string; confirmed: boolean }) => api<{ task: ServiceUpdateTask }>(session!, '/service/update/confirm', { method: 'POST', body: JSON.stringify({ id, confirmed }) }),
    onSuccess: (result) => {
      client.setQueryData(serviceUpdateTaskKey, result)
      if (result.task.state === 'cancelled') clearServiceUpdateMarker()
      else writeServiceUpdateMarker({ targetVersion: result.task.target_version, baseURL: session!.baseURL, task: result.task })
    },
    onSettled: () => { void client.invalidateQueries({ queryKey: serviceUpdateTaskKey }) },
  })
  return { task: query.data?.task, query, mutation, uploadMutation, confirmMutation }
}
