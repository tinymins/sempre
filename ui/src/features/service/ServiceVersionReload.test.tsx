import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useServiceVersionChange } from './ServiceVersionReload'
import { clearServiceUpdateMarker, writeServiceUpdateMarker } from '../../lib/useServiceUpdateTask'

describe('service version reload', () => {
  beforeEach(() => vi.useFakeTimers())

  afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllGlobals()
    clearServiceUpdateMarker(); sessionStorage.clear()
  })

  it('waits for both target service and UI versions before completing', async () => {
    writeServiceUpdateMarker({ targetVersion: '2.0.12' })
    vi.stubGlobal('fetch', vi.fn()
      .mockResolvedValueOnce(Response.json({ version: '2.0.12', ui: { id: 'old', version: '2.0.0' } }))
      .mockResolvedValueOnce(Response.json({ version: '2.0.12', ui: null }))
      .mockResolvedValueOnce(Response.json({ version: '2.0.12', ui: { id: 'new', version: '2.0.12' } })))
    const onChange = vi.fn()
    renderHook(() => useServiceVersionChange(onChange))
    await act(async () => undefined)
    expect(onChange).not.toHaveBeenCalled()
    await act(async () => vi.advanceTimersByTimeAsync(5000))
    expect(onChange).not.toHaveBeenCalled()
    await act(async () => vi.advanceTimersByTimeAsync(5000))
    expect(onChange).toHaveBeenCalledWith('2.0.12', { id: 'new', version: '2.0.12' })
  })
})
