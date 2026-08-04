import { useQuery } from '@tanstack/react-query'
import type {
  ProxyDebugFormat,
  ProxySourceDebugInput,
} from '@acme/types'
import {
  ProxyDebugStepSchema,
  ProxyNodeTraceOutputSchema,
  ProxyPreviewOutputSchema,
  ProxySourceDebugStepSchema,
} from '@acme/types'
import { api, streamRequest } from '@/lib/api'
import { useSession } from '@/lib/session'

export interface PreviewNode {
  name: string
  type: string
  server: string
  port: number
  sourceIndex: number
  sourceUrl: string
  raw: Record<string, unknown>
  filtered?: boolean
  filteredBy?: string
}

function useApiSession() {
  const { session } = useSession()
  if (!session) throw new Error('Sempre session is unavailable')
  return session
}

function streamStep(event: string, data: unknown) {
  if (event === 'error') {
    const message = typeof data === 'object' && data && 'message' in data ? String(data.message) : 'Subscription debug stream failed'
    throw new Error(message)
  }
  return data
}

export const proxyApi = {
  previewNodes: {
    useQuery(input: { id: string; format: string }, options?: { enabled?: boolean }) {
      const session = useApiSession()
      return useQuery({
        queryKey: ['subscriptions', input.id, 'preview-nodes', input.format],
        queryFn: async () => ProxyPreviewOutputSchema.parse(await api<{ nodes: PreviewNode[] }>(session, `/subscriptions/${input.id}/preview-nodes`, { method: 'POST', body: JSON.stringify({ format: input.format }) })),
        enabled: options?.enabled ?? true,
      })
    },
  },
  traceNode: {
    useQuery(input: { id: string; format: string; nodeName: string }, options?: { enabled?: boolean }) {
      const session = useApiSession()
      return useQuery({
        queryKey: ['subscriptions', input.id, 'trace-node', input.format, input.nodeName],
        queryFn: async () => ProxyNodeTraceOutputSchema.parse(await api<unknown>(session, `/subscriptions/${input.id}/trace-node`, { method: 'POST', body: JSON.stringify({ format: input.format, name: input.nodeName }) })),
        enabled: options?.enabled ?? true,
      })
    },
  },
  debugSource: {
    async stream(input: ProxySourceDebugInput, onStep: (step: ReturnType<typeof ProxySourceDebugStepSchema.parse>) => void, signal?: AbortSignal) {
      const sessionValue = sessionStorage.getItem('sempre.session.v1')
      if (!sessionValue) throw new Error('Sempre session is unavailable')
      const session = JSON.parse(sessionValue)
      await streamRequest(session, '/subscriptions/source/debug', input, (event, data) => onStep(ProxySourceDebugStepSchema.parse(streamStep(event, data))), signal)
    },
  },
  debugSubscription: {
    async stream(input: { id: string; format: ProxyDebugFormat }, onStep: (step: ReturnType<typeof ProxyDebugStepSchema.parse>) => void, signal?: AbortSignal) {
      const sessionValue = sessionStorage.getItem('sempre.session.v1')
      if (!sessionValue) throw new Error('Sempre session is unavailable')
      const session = JSON.parse(sessionValue)
      await streamRequest(session, `/subscriptions/${input.id}/debug`, { format: input.format }, (event, data) => onStep(ProxyDebugStepSchema.parse(streamStep(event, data))), signal)
    },
  },
}

export const userApi = {
  getProfile: { useQuery: () => ({ data: { id: 'local', name: 'Local', email: '' } }) },
}
