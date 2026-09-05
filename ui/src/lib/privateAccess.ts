import type { PrivateAccessStatus } from './types'

export type PrivateAccessMode = 'direct' | 'wireguard' | 'mixed' | 'unknown' | 'inactive'

export function privateAccessMode(status?: PrivateAccessStatus): PrivateAccessMode | null {
  if (!status?.connectors.length) return null
  const modes = new Set(status.connectors.map((connector) => connector.mode))
  if (modes.size > 1) return 'mixed'
  return status.connectors[0].mode
}

export function privateAccessTone(mode: PrivateAccessMode): 'green' | 'blue' | 'orange' | 'default' {
  if (mode === 'direct') return 'green'
  if (mode === 'wireguard') return 'blue'
  if (mode === 'unknown' || mode === 'mixed') return 'orange'
  return 'default'
}
