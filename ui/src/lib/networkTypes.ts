export interface NetworkSettings {
  schema: number
  revision: number
  mode: 'local' | 'gateway'
  gateway_capture_host: boolean
  automatic_switching: boolean
  known_networks: KnownNetwork[]
}

export interface KnownNetwork {
  id: string
  name: string
  gateway_mac: string
  disable_proxy: boolean
}

export interface CurrentNetwork {
  supported: boolean
  name: string
  addresses: string[]
  gateway?: string
  gateway_mac?: string
}

export interface NetworkAutomationStatus {
  enabled: boolean
  active: boolean
  path: 'direct' | 'proxy' | 'unknown' | 'inactive'
  network_id?: string
  network_name?: string
  interface?: string
  gateway?: string
  gateway_mac?: string
  probe_error?: string
}

export interface NetworkSettingsResponse {
  settings: NetworkSettings
  current: CurrentNetwork
  platform: string
  gateway_available: boolean
}
