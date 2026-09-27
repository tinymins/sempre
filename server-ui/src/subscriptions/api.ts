import { serverRequest } from '../server-api'
import type { CustomNode, Subscription, SubscriptionDefaults, SubscriptionDraft, SubscriptionSource, UserBrief } from './types'
import type { AccessStats, DraftDebugResult, NodeTraceResult, PreviewNode, SourceDebugResult, Target } from './diagnostic-types'
import type { DebugStage } from './diagnostic-types'
import { readDebugStream } from './debug-stream'

const base = '/subscriptions'

export const subscriptionApi = {
  list: () => serverRequest<Subscription[]>(base),
  get: (id: string) => serverRequest<Subscription>(`${base}/${encodeURIComponent(id)}`),
  create: (draft: SubscriptionDraft) => serverRequest<Subscription>(base, {
    method: 'POST', body: JSON.stringify(draft),
  }),
  update: (id: string, draft: Partial<SubscriptionDraft> & { confirmShareAssignedNodes?: boolean }, updatedAt: string) => serverRequest<Subscription>(`${base}/${encodeURIComponent(id)}`, {
    method: 'PATCH', headers: { 'If-Match': updatedAt }, body: JSON.stringify(draft),
  }),
  remove: (id: string) => serverRequest<void>(`${base}/${encodeURIComponent(id)}`, { method: 'DELETE' }),
  clearCache: (id: string) => serverRequest<{ cleared: number }>(`${base}/clear-cache`, { method: 'POST', body: JSON.stringify({ id }) }),
  defaults: () => serverRequest<SubscriptionDefaults>('/subscription-defaults'),
  users: () => serverRequest<UserBrief[]>('/users'),
  targets: () => serverRequest<Target[]>('/targets'),
  stats: (id: string, page = 1, pageSize = 20) => serverRequest<AccessStats>(`${base}/${encodeURIComponent(id)}/stats?page=${page}&pageSize=${pageSize}`),
  preview: (id: string, target: Target) => serverRequest<{ nodes: PreviewNode[] }>(`${base}/${encodeURIComponent(id)}/preview-nodes`, {
    method: 'POST', body: JSON.stringify({ target }),
  }),
  trace: (id: string, target: Target, name: string) => serverRequest<NodeTraceResult>(`${base}/${encodeURIComponent(id)}/trace-node`, {
    method: 'POST', body: JSON.stringify({ target, name }),
  }),
  debug: (draft: SubscriptionDraft, target: Target, signal: AbortSignal, onStage: (stage: DebugStage) => void, subscriptionId?: string) => readDebugStream<DraftDebugResult>(`${base}/debug`, { draft, target, ...(subscriptionId ? { subscriptionId } : {}) }, signal, onStage),
  debugSaved: (id: string, target: Target, signal: AbortSignal, onStage: (stage: DebugStage) => void) => readDebugStream<DraftDebugResult>(`${base}/${encodeURIComponent(id)}/debug`, { target }, signal, onStage),
  debugSource: (source: SubscriptionSource, mode: 'bypass-cache' | 'production', signal: AbortSignal, onStage: (stage: DebugStage) => void, saved?: { id: string; index: number }) => readDebugStream<SourceDebugResult>(`${base}/debug-source`, {
    url: source.url, ua: source.fetchUa, prefix: source.prefix, cacheTtlMinutes: source.cacheTtlMinutes, fetchMode: source.fetchMode, mode, ...(mode === 'production' && saved ? { subscriptionId: saved.id, sourceIndex: saved.index } : {}),
  }, signal, onStage),
  customNodes: () => serverRequest<CustomNode[]>('/custom-nodes'),
  customNode: (id: string) => serverRequest<CustomNode>(`/custom-nodes/${encodeURIComponent(id)}`),
  createCustomNode: (input: { content: string; authorizedUserIds: string[]; assignedSubscribeIds: string[] }) => serverRequest<CustomNode>('/custom-nodes', {
    method: 'POST', body: JSON.stringify(input),
  }),
  updateCustomNode: (id: string, input: Partial<{ content: string; authorizedUserIds: string[]; assignedSubscribeIds: string[]; confirmUnassignEnabled: boolean }>) => serverRequest<CustomNode>(`/custom-nodes/${encodeURIComponent(id)}`, {
    method: 'PATCH', body: JSON.stringify(input),
  }),
  removeCustomNode: (id: string) => serverRequest<void>(`/custom-nodes/${encodeURIComponent(id)}`, { method: 'DELETE' }),
}
