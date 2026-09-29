import { clearInheritedValues, createSource, normalizeSource, type EditorSource, type ConfigDraft } from '@acme/subscription-editor'
export type SubscriptionSource = EditorSource

export interface SubscriptionDraft extends ConfigDraft {
  remark: string | null
  logLevel: 'off' | 'error' | 'warn' | 'info' | 'debug'
  subscribeItems: SubscriptionSource[] | null
  subscribeUrl: string | null
  authorizedUserIds: string[]
  cacheTtlMinutes: number | null
}

export interface Subscription extends SubscriptionDraft {
  id: string
  userId: string
  url: string
  creator: { id: string; name: string; email: string }
  cachedNodeCount: number
  accessCount: number
  lastAccessAt: string | null
  createdAt: string
  updatedAt: string
  canEdit: boolean
  canDelete: boolean
  canManageAuthorization: boolean
  assignedCustomNodes: { id: string; name: string; proxyType: string; server: string; port: number; enabled: boolean; position: number }[]
}

export interface CustomNode {
  id: string
  userId: string
  content: string
  name: string
  proxyType: string
  server: string
  port: number
  authorizedUserIds: string[]
  creator: UserBrief
  assignments: { subscribeId: string; remark: string | null; enabled: boolean; position: number }[]
  createdAt: string
  updatedAt: string
  canEdit: boolean
  canManageAuthorization: boolean
}

export interface SubscriptionDefaults {
  ruleList: string
  group: string
  filter: string
  customConfig: string
  dnsConfig: string
}

export interface UserBrief {
  id: string
  name: string
  email: string
}

export function emptyDraft(): SubscriptionDraft {
  return {
    remark: null, logLevel: 'info',
    subscribeItems: [createSource()],
    subscribeUrl: null,
    ruleList: null, useSystemRuleList: true,
    group: null, useSystemGroup: true,
    filter: null, useSystemFilter: true,
    servers: null,
    customConfig: null, useSystemCustomConfig: true,
    dnsConfig: null, useSystemDnsConfig: true,
    privateAccessConfig: null,
    authorizedUserIds: [], cacheTtlMinutes: null, selectedCustomNodeIds: [],
  }
}

export function draftFromSubscription(value: Subscription): SubscriptionDraft {
  return clearInheritedValues({
    remark: value.remark, logLevel: value.logLevel,
    subscribeItems: value.subscribeItems?.map(normalizeSource) ?? null,
    subscribeUrl: value.subscribeUrl,
    ruleList: value.ruleList, useSystemRuleList: value.useSystemRuleList,
    group: value.group, useSystemGroup: value.useSystemGroup,
    filter: value.filter, useSystemFilter: value.useSystemFilter,
    servers: value.servers,
    customConfig: value.customConfig, useSystemCustomConfig: value.useSystemCustomConfig,
    dnsConfig: value.dnsConfig, useSystemDnsConfig: value.useSystemDnsConfig,
    privateAccessConfig: value.privateAccessConfig,
    authorizedUserIds: [...value.authorizedUserIds],
    cacheTtlMinutes: value.cacheTtlMinutes,
    selectedCustomNodeIds: [...value.selectedCustomNodeIds],
  })
}
