export type TrafficDimension = 'device' | 'user' | 'host' | 'outbound' | 'process'

export interface TrafficSettings {
  retention_hours: number
  reset_day: number | null
  max_bytes: number
}

export interface TrafficHistory {
  settings: TrafficSettings
  storage_bytes: number
  totals: Array<{ label: string; download: number; upload: number }>
}
