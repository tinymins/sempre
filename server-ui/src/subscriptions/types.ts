export interface SubscriptionSource {
  enabled: boolean
  url: string
  prefix: string
  remark: string
  cacheTtlMinutes?: number
  fetchUa?: string
  fetchMode?: 'auto' | 'domestic-direct'
}

export interface SubscriptionDraft {
  remark: string | null
  logLevel: 'off' | 'error' | 'warn' | 'info' | 'debug'
  subscribeItems: SubscriptionSource[] | null
  subscribeUrl: string | null
  ruleList: string | null
  useSystemRuleList: boolean
  group: string | null
  useSystemGroup: boolean
  filter: string | null
  useSystemFilter: boolean
  servers: string | null
  customConfig: string | null
  useSystemCustomConfig: boolean
  dnsConfig: string | null
  useSystemDnsConfig: boolean
  privateAccessConfig: string | null
  authorizedUserIds: string[]
  cacheTtlMinutes: number | null
  selectedCustomNodeIds: string[]
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
    subscribeItems: [{ enabled: true, url: '', prefix: '', remark: '', fetchMode: 'auto' }],
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
  return {
    remark: value.remark, logLevel: value.logLevel,
    subscribeItems: value.subscribeItems?.map((item) => ({ ...item })) ?? null,
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
  }
}
