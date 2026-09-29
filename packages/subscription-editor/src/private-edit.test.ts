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

it('preserves unknown connector protocols rather than changing their type on edit', () => {
  const source = '{"connectors":[{"type":"future","settings":{"a":1}}]}'
  expect(parse(patchConnector(source, 0, { tag: 'renamed' })).connectors[0]).toEqual({ type: 'future', settings: { a: 1 }, tag: 'renamed' })
})

it('updates both protocol locations when switching an existing outbound', () => {
  const source = '{"connectors":[{"type":"outbound","outbound":{"type":"vmess","server":"host","tls":{"enabled":true}}}]}'
  const connector = parse(patchConnector(source, 0, { type: 'trojan' })).connectors[0]
  expect(connector.type).toBe('trojan')
  expect(connector.outbound).toEqual({ type: 'trojan', server: 'host', tls: { enabled: true } })
})
