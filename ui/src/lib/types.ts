export interface ApiErrorBody {
  error: { code: string; message: string; details?: unknown }
}

export interface Session {
  baseURL: string
  token: string
  expiresAt: string
  warning?: string
}

export interface RuntimeState {
  state?: string
  pid?: number
  core?: string
  version?: string
  started_at?: string
  restart_count?: number
  last_exit?: string
}

export interface SystemStatus {
  version: string
  commit: string
  date: string
  mode: string
  service: string
  runtime: RuntimeState
  selected?: { core: string; ref: string }
  active?: { core: string; ref: string; version: string; config_hash: string }
  pending: boolean
  last_error?: string
  web: { listen: string; local_url: string; password_set: boolean; password_warning: boolean }
  ui: { installed: boolean; metadata?: UIMetadata }
  capabilities: Record<string, boolean>
}

export interface Overview {
  core: string
  version: string
  mode?: string
  connections: number
  download: number
  upload: number
}

export interface Latency { time: string; delay: number }
export interface ProxyNode {
  name: string
  type: string
  now?: string
  all?: string[]
  udp?: boolean
  history?: Latency[]
}
export interface ProxyProvider {
  name: string
  type?: string
  vehicle_type?: string
  updated_at?: string
  proxies: ProxyNode[]
}
export interface Rule { type: string; payload: string; proxy: string; size?: number }
export interface ConnectionMetadata {
  network?: string
  type?: string
  source_ip?: string
  destination_ip?: string
  source_port?: string
  destination_port?: string
  host?: string
  dns_mode?: string
  process?: string
  process_path?: string
  inbound_user?: string
}
export interface Connection {
  id: string
  metadata: ConnectionMetadata
  chains: string[]
  rule?: string
  rule_payload?: string
  download: number
  upload: number
  start?: string
}
export interface ConnectionSnapshot { download_total: number; upload_total: number; connections: Connection[] }

export interface CoreInstallation {
  core: string
  version: string
  channels: string[]
  installation: { explicit: boolean; digest: string; source: string; installed_at: string }
}
export interface CoresResponse {
  supported: string[]
  installed: CoreInstallation[]
  selected?: { core: string; ref: string }
  active?: { core: string; ref: string; version: string; config_hash: string }
}
export interface Subscription {
  url?: string
  interval: string
  last_check?: string
  last_change?: string
  last_result?: string
}
export interface UIManifest { schema: number; name: string; version: string; entry: string; api: { major: number } }
export interface UIMetadata {
  manifest: UIManifest
  source_type: string
  source: string
  sha256: string
  installed_at: string
}

export interface RuntimeEvent<T = unknown> {
  topic: string
  timestamp: string
  sequence: number
  data?: T
  error?: string
}
