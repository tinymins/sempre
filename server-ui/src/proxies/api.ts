import { serverRequest } from '../server-api'

export interface SubscriptionProxy {
  id: string
  name: string
  customNodeId: string | null
  engine: 'sing-box' | 'xray'
  protocol: string
  enabled: boolean
  status: string
  revision: number
  activeRevision: number | null
  lastError: string | null
  checkedAt: string | null
  canEdit: boolean
  referenceCount: number
}
export interface ProxyInput { name: string; content?: string; customNodeId?: string | null; engine: 'sing-box' | 'xray' | null; enabled: boolean }
export const proxyApi = {
  list: () => serverRequest<{ configured: boolean; items: SubscriptionProxy[] }>('/subscription-proxies'),
  edit: (id: string) => serverRequest<SubscriptionProxy & { content: string }>(`/subscription-proxies/${id}`),
  save: (id: string | null, input: ProxyInput) => serverRequest<SubscriptionProxy>(`/subscription-proxies${id ? `/${id}` : ''}`, { method: id ? 'PATCH' : 'POST', body: JSON.stringify(input) }),
  remove: (id: string) => serverRequest<void>(`/subscription-proxies/${id}`, { method: 'DELETE' }),
  test: (id: string) => serverRequest<{ ok: boolean; elapsedMs: number }>(`/subscription-proxies/${id}/test`, { method: 'POST' }),
}
