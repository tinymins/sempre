import { describe, expect, it } from 'vitest'
import { emptyConnector, parseConfig, serializeConfig } from './PrivateAccessConfig'

describe('PrivateAccessConfig tunnel forwarding', () => {
  it('round-trips the managed tunnel forward reference', () => {
    const connector = { ...emptyConnector(), tag: 'hz', tunnelForwardId: 'hz-wg' }
    const serialized = serializeConfig(true, [connector])
    expect(JSON.parse(serialized).connectors[0]).toMatchObject({ type: 'wireguard', tunnel_forward_id: 'hz-wg' })
    expect(parseConfig(serialized).connectors[0].tunnelForwardId).toBe('hz-wg')
  })
})
