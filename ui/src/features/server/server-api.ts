import type { CustomNode, SubscriptionProfile, SubscriptionTarget } from '../../lib/types'

const SESSION_KEY = 'sempre.server.session.v1'

export interface ServerSession {
  token: string
  expiresAt: string
  user: { id: string; email: string }
}

export interface ServerProfile {
  id: string
  owner_id: string
  revision: number
  name: string
  document: SubscriptionProfile
  role: 'owner' | 'editor' | 'viewer'
  updated_at: string
}

export interface ServerShare {
  id: string
  token_prefix: string
  url?: string
  enabled: boolean
  created_at: string
}

export interface ServerMember {
  user_id: string
  email: string
  role: 'viewer' | 'editor'
}

export interface ServerCustomNode extends CustomNode {
  owner_id: string
  authorized_user_ids: string[]
}

export interface ServerProfileStats {
  total_accesses: number
  today_accesses: number
  last_access_at?: string
  by_target: { target: string; count: number }[]
  recent_accesses: { target: string; user_agent: string; created_at: string }[]
}

export interface ServerCompileResult {
  content: string
  node_count: number
  artifact_hash: string
  field_diffs: { node: string; represented: boolean; dropped?: string[]; warnings?: string[] }[]
  diagnostics: { level: string; source_id?: string; message: string }[]
}

export interface ServerRefreshSettings {
  enabled: boolean
  interval_minutes: number
  targets: string[]
  next_refresh_at?: string
  last_refresh_at?: string
  last_refresh_status: 'never' | 'running' | 'success' | 'failed'
  last_refresh_error?: string
}

export interface ServerPreviewNode {
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

export interface ServerSourceTestResult {
  source_id: string
  source_type: string
  format: string
  byte_count: number
  content_hash: string
  node_count: number
  discarded_node_count: number
  diagnostics: string[]
}

export function loadServerSession(): ServerSession | null {
  try {
    const value = localStorage.getItem(SESSION_KEY)
    if (!value) return null
    const session = JSON.parse(value) as ServerSession
    if (!session.token || new Date(session.expiresAt) <= new Date()) return null
    return session
  } catch {
    return null
  }
}

export function saveServerSession(session: ServerSession | null) {
  if (session) localStorage.setItem(SESSION_KEY, JSON.stringify(session))
  else localStorage.removeItem(SESSION_KEY)
}

export async function serverAuthenticate(mode: 'login' | 'register', email: string, password: string) {
  const response = await fetch(`/api/v1/auth/${mode}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
    body: JSON.stringify({ email, password }),
  })
  const result = await serverResponse<{ token: string; expires_at: string; user: ServerSession['user'] }>(response)
  return { token: result.token, expiresAt: result.expires_at, user: result.user }
}

export async function serverAPI<T>(session: ServerSession, path: string, init: RequestInit = {}) {
  const headers = new Headers(init.headers)
  headers.set('Authorization', `Bearer ${session.token}`)
  headers.set('Accept', 'application/json')
  if (init.body) headers.set('Content-Type', 'application/json')
  const response = await fetch(`/api/v1${path}`, { ...init, headers })
  if (response.status === 401) saveServerSession(null)
  return serverResponse<T>(response)
}

export async function serverLogout(session: ServerSession) {
  await serverAPI<void>(session, '/auth/logout', { method: 'DELETE' })
  saveServerSession(null)
}

export async function serverTargets() {
  const response = await fetch('/api/v1/targets', { headers: { Accept: 'application/json' } })
  return serverResponse<SubscriptionTarget[]>(response)
}

async function serverResponse<T>(response: Response): Promise<T> {
  if (response.ok) {
    if (response.status === 204) return undefined as T
    return response.json() as Promise<T>
  }
  try {
    const body = await response.json() as { error?: { message?: string } }
    throw new Error(body.error?.message || `HTTP ${response.status}`)
  } catch (error) {
    if (error instanceof Error && !error.message.startsWith('Unexpected')) throw error
    throw new Error(`HTTP ${response.status}`, { cause: error })
  }
}

export function newServerProfile(name: string): SubscriptionProfile {
  const secret = randomSecret()
  return {
    id: crypto.randomUUID(), revision: 1, name, mode: 'local', log_level: 'info',
    editor: { rule_list: '{}', group: '[]', filter: '[]', custom_config: '[]', dns_config: '', private_access_config: '', servers: '[]' },
    sources: [], custom_node_ids: [], groups: [], rules: [], rule_providers: [], filters: [], core_overrides: {},
    local_proxy: { socks_port: 1080, http_port: 1081, username: 'sempre', password: secret },
    transparent_proxy: {
      mode: 'tun-router', capture_host: false, lan_interfaces: [], route_exclusions: [], interface_mode: 'all', interfaces: [],
      auto_exclude_local_routes: true, auto_exclude_vpn_routes: true, tun: { interface_name: 'sempre-tun' },
      tproxy: { listen_port: 7893, dns_listen_port: 1053 }, ebpf: { wan_interface: 'auto', auto_config_kernel_parameter: false },
    },
    management_api: { external_controller: '0.0.0.0:9090', secret: randomSecret(), allow_origins: [], allow_private_network: false },
    use_system_groups: true, use_system_rules: true, use_system_filters: true, use_system_dns: true, use_system_custom_config: true,
    last_runtime_validated: false,
  }
}

function randomSecret() {
  const bytes = crypto.getRandomValues(new Uint8Array(32))
  return btoa(String.fromCharCode(...bytes)).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', '')
}
