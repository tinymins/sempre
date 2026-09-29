import { expect, it } from 'vitest'
import { parse } from 'jsonc-parser'
import { updateDnsField } from './dns-model'

it('edits DNS without removing a format override or unknown shared values', () => {
  const source = '{ // keep this\n"shared":{"future":1},"overrides":{"clash":{"enable":true}}}'
  const next = updateDnsField(source, 'remoteDns', '1.1.1.1')
  expect(parse(next)).toEqual({ shared: { future: 1, remoteDns: '1.1.1.1' }, overrides: { clash: { enable: true } } })
  expect(next).toContain('// keep this')
})
