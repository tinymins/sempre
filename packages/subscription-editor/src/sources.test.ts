import { expect, it } from 'vitest'
import { moveSource, normalizeSource, sourceText } from './sources'

it('keeps URL and RAW sources together in user-defined order with stable identities', () => {
  const sources = [normalizeSource({ id: 'a', url: 'https://a.test' }), normalizeSource({ id: 'b', type: 'raw', content: 'proxies: []' }), normalizeSource({ id: 'c', url: 'https://c.test' })]
  const moved = moveSource(sources, 1, 0)
  expect(moved.map(source => source.id)).toEqual(['b', 'a', 'c'])
  expect(moved.map(sourceText)).toEqual(['proxies: []', 'https://a.test', 'https://c.test'])
  expect(sources.map(source => source.id)).toEqual(['a', 'b', 'c'])
})
