export type TrafficDimension = 'device' | 'user' | 'host' | 'outbound' | 'process'

export interface TrafficSettings {
  window_hours: number
  retention_hours: number | null
  reset_day: number | null
  retention_months: number | null
  max_bytes: number | null
}

export interface TrafficHistory {
  settings: TrafficSettings
  storage_bytes: number
  totals: Array<{ label: string; download: number; upload: number }>
}
