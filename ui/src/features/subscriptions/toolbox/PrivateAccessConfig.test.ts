import { describe, expect, it } from 'vitest'
import { emptyConnector, parseConfig, serializeConfig } from './PrivateAccessConfig'

describe('PrivateAccessConfig tunnel forwarding', () => {
  it('round-trips the managed transport endpoint reference', () => {
    const connector = { ...emptyConnector(), tag: 'hz', transportEndpointRef: 'hz-wg' }
    const serialized = serializeConfig(true, [connector])
    expect(JSON.parse(serialized).connectors[0]).toMatchObject({ type: 'wireguard', transport_endpoint_ref: 'hz-wg' })
    expect(parseConfig(serialized).connectors[0].transportEndpointRef).toBe('hz-wg')
  })

  it('migrates the legacy tunnel reference on the next edit', () => {
    const legacy = JSON.stringify({ enabled: true, connectors: [{ type: 'wireguard', tunnel_forward_id: 'hz-wg', endpoint: { peers: [{}] } }] })
    const parsed = parseConfig(legacy)
    const serialized = serializeConfig(parsed.enabled, parsed.connectors)
    expect(parsed.connectors[0].transportEndpointRef).toBe('hz-wg')
    expect(JSON.parse(serialized).connectors[0]).toMatchObject({ transport_endpoint_ref: 'hz-wg' })
  })
})
