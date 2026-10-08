import { describe, expect, it } from 'vitest'
import { parseWireGuardImport, WireGuardImportError } from './WireGuardImport'

describe('OpenWrt WireGuard import', () => {

  it('rejects multiple peers instead of silently importing the wrong one', () => {
    expect(() => parseWireGuardImport(`[Interface]\nPrivateKey=x=\nAddress=10.0.0.2/32\n[Peer]\nPublicKey=y=\nAllowedIPs=10.0.0.0/8\nEndpoint=host:51820\n[Peer]\nPublicKey=z=`)).toThrowError(
      expect.objectContaining<Partial<WireGuardImportError>>({ code: 'multiplePeers' }),
    )
  })
})
