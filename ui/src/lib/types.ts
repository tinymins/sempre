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
  repository?: string
  version?: string
  started_at?: string
  restart_count?: number
  last_exit?: string
  last_error?: string
  last_transition?: string
  ref?: string
  config_hash?: string
}

export interface RuntimeActionAvailability {
  allowed: boolean
  reason?: string
}

export interface ManagedRuntimeDeployment {
  core: string
  repository?: string
  ref: string
  version: string
  exact_reference: string
  config_hash: string
}

export interface ManagedRuntimeFailure {
  stage: string
  error: string
  occurred_at: string
  failed?: ManagedRuntimeDeployment
  rolled_back_to?: ManagedRuntimeDeployment
}

export interface ManagedRuntimeStatus {
  desired_state: 'running' | 'stopped'
  runtime_state: 'idle' | 'stopped' | 'starting' | 'running' | 'stopping' | 'restarting' | 'failed'
  active: ManagedRuntimeDeployment | null
  target?: ManagedRuntimeDeployment
  pid: number
  started_at: string | null
  uptime_seconds: number
  restart_count: number
  pending: boolean
  last_transition: string | null
  last_exit?: string
  last_error?: string
  last_failure?: ManagedRuntimeFailure
  dns_frontend?: {
    enabled: boolean
    running: boolean
    core_dns_healthy: boolean
    mode: 'fake-ip' | 'real-ip' | ''
    core_upstream: string
    original_upstreams: string[]
    domestic_domain_source: string
    domestic_domain_sha256: string
    domestic_domain_count: number
    last_error?: string
  }
  actions: {
    start: RuntimeActionAvailability
    stop: RuntimeActionAvailability
    restart: RuntimeActionAvailability
  }
}

export interface SystemStatus {
  version: string
  commit: string
  date: string
  mode: string
  service_memory: number
  service: string
  desired_state: 'running' | 'stopped'
  runtime: RuntimeState
  selected?: { core: string; repository?: string; ref: string }
  active?: { core: string; repository?: string; ref: string; version: string; config_hash: string }
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

export interface NetworkTestResult {
  id: string
  name: string
  region: 'domestic' | 'foreign'
  category: 'reachability' | 'ip'
  url: string
  ok: boolean
  latency_ms: number
  http_status?: number
  ip?: string
  ip_metadata?: IpMetadata
  detail?: string
}

export interface IpMetadata {
  country_code?: string
  country?: string
  region?: string
  city?: string
  asn?: number
  asn_organization?: string
  isp?: string
  organization?: string
}

export interface NetworkTestReport {
  checked_at: string
  results: NetworkTestResult[]
}

export interface CoreInstallation {
  core: string
  repository: string
  reference: string
  official: boolean
  version: string
  channels: string[]
  installation: { explicit: boolean; digest: string; source: string; installed_at: string }
}
export interface CoresResponse {
  supported: string[]
  installed: CoreInstallation[]
	catalog?: CoreDefinition[]
  selected?: { core: string; repository?: string; ref: string }
  active?: { core: string; repository?: string; ref: string; version: string; config_hash: string }
}

export interface CoreDefinition {
	id: string
	name: string
	stability: 'stable' | 'experimental'
	compiler_format: string
	control_protocol?: 'clash-rest' | 'grpc'
	platforms: string[]
}
export interface Subscription {
  url?: string
  interval: string
  last_check?: string
  last_change?: string
  last_result?: string
}

export interface SubscriptionSource {
  id: string
  type: 'url' | 'raw'
  enabled: boolean
  url?: string
  content?: string
  prefix?: string
  remark?: string
  user_agent?: string
  fetch_mode?: 'auto' | 'domestic-direct'
  cache_ttl_minutes?: number
  snapshot_hash?: string
  fetched_at?: string
  last_status?: string
  last_error?: string
}

export interface ProxyGroup {
  name: string
  type: string
  proxies?: string[]
  include_all?: boolean
  readonly?: boolean
  url?: string
  interval?: number
  tolerance?: number
	default?: string
}

export interface TransparentProxyConfig {
	mode: 'tun-router' | 'tproxy' | 'ebpf-router' | 'disabled'
	capture_host: boolean
	lan_interfaces: string[]
	route_exclusions: string[]
	interface_mode: 'all' | 'include' | 'exclude'
	interfaces: string[]
	auto_exclude_local_routes: boolean
	auto_exclude_vpn_routes: boolean
	tun: {
		interface_name: string
		address?: string
	}
	tproxy: {
		listen_port: number
		dns_listen_port: number
	}
	ebpf: {
		wan_interface: string
		auto_config_kernel_parameter: boolean
	}
}

export interface LocalProxyConfig {
	socks_port: number
	http_port: number
	username: string
	password: string
}

export interface ManagementAPIConfig {
	external_controller?: string
	secret?: string
	external_ui?: string
	allow_origins: string[]
	allow_private_network: boolean
}

export interface CoreProtocolCapability {
	protocol: string
	transports: string[]
	security: string[]
	minimum_version?: string
}

export interface CoreCapabilities {
	features: string[]
	enum_values: Record<string, string[]>
	protocols: CoreProtocolCapability[]
}

export interface SubscriptionConfigurationContext {
	key: string
	target?: { core: string; version: string; compiler_target: SubscriptionTarget; key: string }
	running?: { core: string; version: string }
	platform: string
	capabilities: CoreCapabilities
}

export interface LinuxNetworkInventory {
	supported: boolean
	default_interface?: string
	recommended_lan_interfaces: string[]
	local_prefixes: string[]
	vpn_prefixes: string[]
	occupied_prefixes: string[]
	interfaces: Array<{
		name: string
		index: number
		kind: string
		up: boolean
		default_route: boolean
		addresses: string[]
	}>
}

export interface GatewayConfig {
  schema: number
  topology: 'local-pve' | 'remote-pve'
  lan: {
    interface: string
    gateway_cidr: string
    wan_interface: string
    nat_enabled: boolean
  }
  dhcp: {
    enabled: boolean
    range_start: string
    range_end: string
    lease_time: string
    domain?: string
    reservations: Array<{ mac: string; ip: string; hostname?: string }>
  }
  pve: {
    host?: string
    port?: number
    user?: string
    key_path?: string
    fingerprint?: string
    apply_persistent: boolean
  }
}

export interface GatewayLease {
  mac: string
  ip: string
  hostname?: string
  expires_at?: string | null
  reserved: boolean
}

export interface GatewayStatus {
  config: GatewayConfig
  runtime: {
    dhcp_running: boolean
    started_at?: string | null
    dhcp_leases: GatewayLease[]
    last_error?: string
  }
  inventory: LinuxNetworkInventory
  validation_errors: string[]
  transparent_proxy?: TransparentProxyConfig
  host_plan_available: boolean
}

export interface GatewayHostPlan {
  topology: string
  summary: string
  warnings: string[]
  commands: string[]
  persistent_commands: string[]
  apply_by_ssh: boolean
  output?: string[]
}

export interface NetworkSettings {
  schema: number
  revision: number
  mode: 'local' | 'gateway'
  gateway_capture_host: boolean
}

export interface NetworkSettingsResponse {
  settings: NetworkSettings
  platform: string
  gateway_available: boolean
}

export interface TunnelForward {
  id: string
  name: string
  listen_port: number
  remote_host: string
  remote_port: number
  timeout_seconds: number
}

export interface TunnelInstance {
  id: string
  name: string
  desired_state: 'running' | 'stopped'
  server_url: string
  dns_resolvers: string[]
  prefer_ipv4: boolean
  websocket_ping: string
  connection_retry_max_backoff: string
  upgrade_path_prefix?: string
  forwards: TunnelForward[]
}

export interface TunnelConfig {
  schema: number
  instances: TunnelInstance[]
}

export interface TunnelStatus {
  config: TunnelConfig
  binary: { version: string; installed: boolean }
  instances: Array<{ id: string; state: string; restart_count: number; started_at?: string; last_error?: string; log_path: string }>
  forwards: Array<{ instance_id: string; instance_name: string; forward_id: string; forward_name: string; host: string; port: number }>
}

export interface RuleProvider { tag: string; url: string; outbound?: string; format?: string; behavior?: string }

export interface SubscriptionDefaults {
  groups: ProxyGroup[]
  rule_providers: RuleProvider[]
  filters: string[]
  rules: string[]
  dns: Record<string, unknown>
}

export interface SubscriptionEditorConfig {
  rule_list: string
  group: string
  filter: string
  custom_config: string
  dns_config: string
  private_access_config: string
  servers: string
}

export interface SubscriptionProfile {
  id: string
  revision: number
  name: string
  mode: 'local' | 'remote'
  remote?: {
    manifest_url: string
    edit_url?: string
    server_profile?: string
    server_revision?: number
    artifact_sha256?: string
    target?: string
    node_count?: number
    server_updated_at?: string
    artifact_created_at?: string
    last_synced_at?: string
  }
  remark?: string
  log_level: 'off' | 'error' | 'warn' | 'info' | 'debug'
  editor: SubscriptionEditorConfig
  sources: SubscriptionSource[]
  custom_node_ids: string[]
  groups: ProxyGroup[]
  rules: string[]
  rule_providers: RuleProvider[]
  filters: string[]
  dns?: Record<string, unknown>
  private_access?: Record<string, unknown>
	core_overrides: Record<string, Record<string, unknown>>
	local_proxy?: LocalProxyConfig
	transparent_proxy?: TransparentProxyConfig
	management_api?: ManagementAPIConfig
  use_system_groups: boolean
  use_system_rules: boolean
  use_system_filters: boolean
  use_system_dns: boolean
  use_system_custom_config: boolean
  last_check?: string
  last_change?: string
  last_result?: string
  last_config_hash?: string
  last_runtime_validated: boolean
  last_compiler_target?: string
  last_compiler_warnings?: string[]
}

export interface CustomNode {
  id: string
  name: string
  proxy: Record<string, unknown>
  created_at?: string
  updated_at?: string
}

export interface SubscriptionTarget { core?: string; format: string; version?: string; platform?: string }
export interface SubscriptionCatalogResponse {
  profiles: SubscriptionProfile[]
  active_profile_id: string
  schedule: Subscription
  auto_restart: boolean
  targets: SubscriptionTarget[]
  defaults: SubscriptionDefaults
  editor_defaults: SubscriptionEditorConfig & { by_core?: Record<string, SubscriptionEditorConfig> }
	configuration_context: SubscriptionConfigurationContext
}

export interface SourceResult {
  source: SubscriptionSource
  parse: { format: string; nodes: Array<Record<string, unknown>>; discarded_placeholder_nodes: Array<Record<string, unknown>>; diagnostics: string[] }
  from_cache: boolean
  content_hash: string
  bytes: number
}

export interface FieldOrigin {
  source_key?: string
  source_value?: unknown
  step: string
  transform: string
  reason?: string
  sources?: string[]
}

export interface FieldDiff {
  node: string
  consumed: string[]
  ignored: string[]
  dropped: string[]
  warnings: string[]
  outbound?: Record<string, unknown>
  field_origins?: Record<string, FieldOrigin>
}

export interface RenderResult {
  format: string
  version?: string
  platform?: string
  content: string
  node_count: number
  source_results?: SourceResult[]
  field_diffs?: FieldDiff[]
  node_origins?: Record<string, string>
  warnings?: string[]
  runtime_validated: boolean
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
