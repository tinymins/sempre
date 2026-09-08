import type { NetworkAutomationStatus } from './networkTypes'

export type NetworkAutomationDisplayPath = NetworkAutomationStatus['path'] | 'pending'

export function networkAutomationDisplayPath(status?: NetworkAutomationStatus): NetworkAutomationDisplayPath | null {
  if (!status?.enabled) return null
  if (status.path === 'unknown' && status.gateway_mac) return 'pending'
  return status.path
}
