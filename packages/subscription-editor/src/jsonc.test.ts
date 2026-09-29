import { describe, expect, it } from 'vitest'
import { parse } from 'jsonc-parser'
import { editJsonc, readJsoncObject } from './jsonc'

describe('configuration document editing', () => {
  it('preserves DNS overrides, unknown fields and comments when changing a shared field', () => {
    const source = `{
  // a user explanation
  "shared": { "remoteDns": "8.8.8.8", "futureSetting": false },
  "overrides": { "singbox": { "servers": [] } },
  "extension": { "value": 42 }
}`
    const next = editJsonc(source, ['shared', 'remoteDns'], '1.1.1.1')
    expect(next).toContain('// a user explanation')
    expect(parse(next)).toEqual({
      shared: { remoteDns: '1.1.1.1', futureSetting: false },
      overrides: { singbox: { servers: [] } },
      extension: { value: 42 },
    })
  })

  it('preserves additional WireGuard peers and DNS entries', () => {
    const source = JSON.stringify({ connectors: [{ endpoint: { peers: [{ port: 1 }, { port: 2 }] }, dns: [{ server: 'a' }, { server: 'b' }] }] })
    const next = editJsonc(source, ['connectors', 0, 'endpoint', 'peers', 0, 'port'], 51820)
    expect(parse(next).connectors[0]).toEqual({ endpoint: { peers: [{ port: 51820 }, { port: 2 }] }, dns: [{ server: 'a' }, { server: 'b' }] })
  })

  it('does not turn unfinished or non-object documents into an empty configuration', () => {
    for (const source of ['{ "shared": ', '[]', 'null', 'false']) {
      expect(readJsoncObject(source).error).toBe(true)
      expect(editJsonc(source, ['shared', 'fakeipEnabled'], false)).toBe(source)
    }
  })

  it('distinguishes explicit false, zero, empty and absent values', () => {
    let source = editJsonc('', ['shared', 'fakeipEnabled'], false)
    source = editJsonc(source, ['shared', 'fakeipTtl'], 0)
    source = editJsonc(source, ['shared', 'remoteDetour'], '')
    expect(parse(source).shared).toEqual({ fakeipEnabled: false, fakeipTtl: 0, remoteDetour: '' })
    source = editJsonc(source, ['shared', 'remoteDetour'], undefined)
    expect(parse(source).shared).not.toHaveProperty('remoteDetour')
  })
})
