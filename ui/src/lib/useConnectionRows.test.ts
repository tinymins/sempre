import { describe, expect, it } from 'vitest'
import { mergeConnectionSnapshot } from './useConnectionRows'
import type { Connection } from './types'

const first: Connection = { id: 'a', metadata: {}, chains: [], start: '2026-09-09T00:00:00Z', download: 1024, upload: 512 }

describe('connection snapshots', () => {
  it('measures byte deltas over the actual sampling interval, without estimating the first sample', () => {
    const initial = mergeConnectionSnapshot([], [first], 2000, false)
    expect(initial[0]).toMatchObject({ downloadSpeed: null, uploadSpeed: null })
    const next = mergeConnectionSnapshot(initial, [{ ...first, download: 5120, upload: 2560 }], 4000, false)
    expect(next[0]).toMatchObject({ downloadSpeed: 1024, uploadSpeed: 512 })
    expect(mergeConnectionSnapshot(next, [next[0]], 2000, false)[0]).toMatchObject({ downloadSpeed: 0, uploadSpeed: 0 })
  })

  it('retains vanished connections only when requested and clears stale speeds', () => {
    const initial = mergeConnectionSnapshot([], [first], 2000, true)
    const closed = mergeConnectionSnapshot(initial, [], 2000, true)
    expect(closed).toEqual([{ ...first, closed: true, downloadSpeed: null, uploadSpeed: null }])
    expect(mergeConnectionSnapshot(closed, [], 2000, true)).toEqual(closed)
    expect(mergeConnectionSnapshot(closed, [], 2000, false)).toEqual([])
    expect(mergeConnectionSnapshot(closed, [first], 2000, true)).toEqual(initial)
  })

  it('does not produce negative speeds or compare a replaced connection with its old counters', () => {
    const initial = mergeConnectionSnapshot([], [first], 2000, false)
    expect(mergeConnectionSnapshot(initial, [{ ...first, download: 0, upload: 0 }], 2000, false)[0]).toMatchObject({ downloadSpeed: 0, uploadSpeed: 0 })
    expect(mergeConnectionSnapshot(initial, [first], 0, false)[0].downloadSpeed).toBeNull()
    expect(mergeConnectionSnapshot(initial, [{ ...first, start: '2026-09-09T01:00:00Z' }], 2000, false)[0].downloadSpeed).toBeNull()
  })
})
