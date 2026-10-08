import { expect, it } from 'vitest'
import { parse } from 'jsonc-parser'
import { patchConnector } from './private-edit'

it('preserves other peers, DNS rules, aliases and extension fields during a connector edit', () => {
  const source = '{ // retained\n"enabled":true,"connectors":[{"type":"wireguard","extension":true,"endpoint":{"private_key":"old","peers":[{"public_key":"key","port":1},{"port":2}]},"dns":[{"server":"a"},{"server":"b"}]}]}'
  const next = patchConnector(source, 0, { privateKey: 'new', peerPort: 51820, dnsServer: 'c' })
  const connector = parse(next).connectors[0]
  expect(next).toContain('// retained')
  expect(connector.extension).toBe(true)
  expect(connector.endpoint.private_key).toBe('new')
  expect(connector.endpoint).not.toHaveProperty('privateKey')
  expect(connector.endpoint.peers).toEqual([{ public_key: 'key', port: 51820 }, { port: 2 }])
  expect(connector.dns).toEqual([{ server: 'c' }, { server: 'b' }])
})
