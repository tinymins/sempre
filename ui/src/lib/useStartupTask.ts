import { useEffect } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from './api'
import { useSession } from './session'
import type { ManagedRuntimeStatus } from './types'
import type { StartupTask } from './startupTask'

const taskKey = ['runtime', 'startup-task']

export function useStartupTask() {
  const { session } = useSession()
  const client = useQueryClient()
  const query = useQuery({
    queryKey: taskKey,
    queryFn: () => api<{ task: StartupTask | null }>(session!, '/runtime/startup-task'),
    enabled: Boolean(session),
    refetchInterval: (query) => query.state.data?.task?.state === 'running' ? 500 : 3000,
    refetchIntervalInBackground: true,
  })
  const task = query.data?.task
  const mutation = useMutation({
    mutationKey: taskKey,
    mutationFn: () => api<{ task: StartupTask; status: ManagedRuntimeStatus }>(session!, '/runtime/restart', { method: 'POST' }),
    onMutate: async () => { await client.cancelQueries({ queryKey: taskKey }) },
    onSuccess: (result) => {
      client.setQueryData(taskKey, { task: result.task })
      client.setQueryData(['runtime', 'status'], result.status)
    },
    onSettled: () => { void client.invalidateQueries({ queryKey: taskKey }) },
  })
  useEffect(() => {
    if (!task?.finished_at) return
    void client.invalidateQueries({ queryKey: ['runtime', 'status'] })
    void client.invalidateQueries({ queryKey: ['runtime', 'proxies'] })
    void client.invalidateQueries({ queryKey: ['system'] })
  }, [client, task?.id, task?.finished_at])
  return { task, query, mutation }
}
