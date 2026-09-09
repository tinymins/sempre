import { describe, expect, it } from 'vitest'
import { emptyConnector, parseConfig, serializeConfig } from './PrivateAccessConfig'
import { parseWireGuardImport, WireGuardImportError } from './WireGuardImport'

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

describe('OpenWrt WireGuard import', () => {
  it('maps an exported client configuration to the connector form', () => {
    const imported = parseWireGuardImport(`
[Interface]
PrivateKey = sxxxxxxxxxxxxxxxxxxxxxxxxx2nkQ=
Address = 10.3.8.10/32
# ListenPort not defined
DNS = 10.3.7.1

[Peer]
PublicKey = 0xxxxxxxxxxxxxxxxxxxxxxxxxp2lQkDH4vA81s=
PresharedKey = WxxxxxxxxxxxxxxxxxxxxxxxxxJGRKd1Bmsfqf74=
AllowedIPs = 0.0.0.0/0, ::/0
Endpoint = 14.90.103.42:31088
PersistentKeepAlive = 25
`)

    expect(imported).toEqual({
      address: '10.3.8.10/32',
      privateKey: 'sxxxxxxxxxxxxxxxxxxxxxxxxx2nkQ=',
      peerAddress: '14.90.103.42',
      peerPort: 31088,
      transportEndpointRef: '',
      publicKey: '0xxxxxxxxxxxxxxxxxxxxxxxxxp2lQkDH4vA81s=',
      preSharedKey: 'WxxxxxxxxxxxxxxxxxxxxxxxxxJGRKd1Bmsfqf74=',
      allowedIps: '0.0.0.0/0, ::/0',
      persistentKeepaliveInterval: 25,
      dnsServer: '10.3.7.1',
      dnsServerPort: 53,
    })
  })

  it('supports repeated addresses and a bracketed IPv6 endpoint', () => {
    expect(parseWireGuardImport(`[Interface]\nPrivateKey=x=\nAddress=10.0.0.2/32\nAddress=fd00::2/128\n[Peer]\nPublicKey=y=\nAllowedIPs=10.0.0.0/8\nEndpoint=[2001:db8::1]:51820`)).toMatchObject({
      address: '10.0.0.2/32, fd00::2/128',
      peerAddress: '2001:db8::1',
      peerPort: 51820,
    })
  })

  it('rejects multiple peers instead of silently importing the wrong one', () => {
    expect(() => parseWireGuardImport(`[Interface]\nPrivateKey=x=\nAddress=10.0.0.2/32\n[Peer]\nPublicKey=y=\nAllowedIPs=10.0.0.0/8\nEndpoint=host:51820\n[Peer]\nPublicKey=z=`)).toThrowError(
      expect.objectContaining<Partial<WireGuardImportError>>({ code: 'multiplePeers' }),
    )
  })
})

describe('PrivateAccessConfig home network detection', () => {
  it('round-trips the selected home network IDs', () => {
    const connector = {
      ...emptyConnector(),
      tag: 'home-wg',
      homeNetworkEnabled: true,
      homeNetworkIds: ['d286d2f8-33c5-4f1e-b871-d22a9ba91143'],
    }
    const serialized = serializeConfig(true, [connector])
    expect(JSON.parse(serialized).connectors[0].homeNetwork).toEqual({
      enabled: true,
      networkIds: ['d286d2f8-33c5-4f1e-b871-d22a9ba91143'],
    })
    expect(parseConfig(serialized).connectors[0]).toMatchObject({
      homeNetworkEnabled: true,
      homeNetworkIds: ['d286d2f8-33c5-4f1e-b871-d22a9ba91143'],
    })
  })

  it('preserves network IDs while the switch is disabled', () => {
    const connector = {
      ...emptyConnector(),
      homeNetworkEnabled: false,
      homeNetworkIds: ['d286d2f8-33c5-4f1e-b871-d22a9ba91143'],
    }
    expect(JSON.parse(serializeConfig(true, [connector])).connectors[0].homeNetwork).toEqual({
      enabled: false,
      networkIds: ['d286d2f8-33c5-4f1e-b871-d22a9ba91143'],
    })
  })
})
