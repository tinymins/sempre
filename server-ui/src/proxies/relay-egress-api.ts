import { serverRequest } from '../server-api'

export type RelayOutbound = { type: 'system' } | { type: 'direct' } | { type: 'proxy'; proxyId: string }
export interface RelayEgressPolicy {
  revision: number
  defaultOutbound: RelayOutbound
  hostRules: { host: string; outbound: RelayOutbound }[]
  availableProxies: { id: string; name: string; enabled: boolean; status: string }[]
}
export interface RelayEgressTest {
  ok: boolean
  host: string
  matchedHost: string | null
  outbound: RelayOutbound
  outboundName: string
  httpStatus: number | null
  elapsedMs: number
  bodyBytes: number
  error?: string
}
export const relayEgressApi = {
  get: () => serverRequest<RelayEgressPolicy>('/admin/relay-egress'),
  save: ({ revision, defaultOutbound, hostRules }: RelayEgressPolicy) => serverRequest<RelayEgressPolicy>('/admin/relay-egress', {
    method: 'PUT', body: JSON.stringify({ revision, defaultOutbound, hostRules }),
  }),
  test: (url: string) => serverRequest<RelayEgressTest>('/admin/relay-egress/test', { method: 'POST', body: JSON.stringify({ url }) }),
}
