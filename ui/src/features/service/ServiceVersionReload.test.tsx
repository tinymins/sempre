import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useServiceVersionChange } from './ServiceVersionReload'

describe('service version reload', () => {
  beforeEach(() => vi.useFakeTimers())

  afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllGlobals()
  })

  it('notifies when the service comes back on a different version', async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(Response.json({ version: '2.0.0' }))
      .mockRejectedValueOnce(new TypeError('service restarting'))
      .mockResolvedValueOnce(Response.json({ version: '2.0.10' }))
    vi.stubGlobal('fetch', fetchMock)
    const onChange = vi.fn()

    renderHook(() => useServiceVersionChange(onChange))
    await act(async () => undefined)
    expect(fetchMock).toHaveBeenCalledTimes(1)

    await act(async () => vi.advanceTimersByTimeAsync(5000))
    expect(onChange).not.toHaveBeenCalled()
    await act(async () => vi.advanceTimersByTimeAsync(5000))

    expect(onChange).toHaveBeenCalledTimes(1)
    expect(fetchMock).toHaveBeenLastCalledWith(`${window.location.origin}/api/v1/health`, expect.objectContaining({ cache: 'no-store' }))
  })
})
