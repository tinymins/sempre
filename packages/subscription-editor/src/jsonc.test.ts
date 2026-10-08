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

  it('does not turn unfinished or non-object documents into an empty configuration', () => {
    for (const source of ['{ "shared": ', '[]', 'null', 'false']) {
      expect(readJsoncObject(source).error).toBe(true)
      expect(editJsonc(source, ['shared', 'fakeipEnabled'], false)).toBe(source)
    }
  })
})
