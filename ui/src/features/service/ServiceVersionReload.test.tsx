import { act, renderHook } from '@testing-library/react'
import { QueryClient } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { completeServiceUpdateOnVersionChange, useServiceVersionChange } from './ServiceVersionReload'
import { clearServiceUpdateMarker, serviceUpdateTaskKey, writeServiceUpdateMarker } from '../../lib/useServiceUpdateTask'

describe('service version reload', () => {
  beforeEach(() => vi.useFakeTimers())

  afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllGlobals()
    clearServiceUpdateMarker(); sessionStorage.clear()
  })

  it('completes an active update before the console reloads', () => {
    const client = new QueryClient()
    writeServiceUpdateMarker({ targetVersion: '2.0.10' })
    client.setQueryData(serviceUpdateTaskKey, { task: { id: 'update-1', state: 'running', stage: 'installing', current_version: '2.0.0', target_version: '2.0.10', downloaded_bytes: 100, total_bytes: 100, bytes_per_second: 10, started_at: '2026-09-07T00:00:00Z', updated_at: '2026-09-07T00:00:01Z' } })

    expect(completeServiceUpdateOnVersionChange(client, 'v2.0.10')).toBe(true)
    expect(client.getQueryData<{ task: { state: string; stage: string } }>(serviceUpdateTaskKey)?.task).toMatchObject({ state: 'succeeded', stage: 'completed' })
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

  it('keeps an active update visible when a different version returns', () => {
    writeServiceUpdateMarker({ targetVersion: '2.0.11' })
    expect(completeServiceUpdateOnVersionChange(new QueryClient(), '2.0.10')).toBe(true)
  })

  it('checks the connected instance and recognizes the active update target', async () => {
    writeServiceUpdateMarker({ targetVersion: '2.0.11' })
    const fetchMock = vi.fn().mockResolvedValue(Response.json({ version: '2.0.11' }))
    vi.stubGlobal('fetch', fetchMock)
    const onChange = vi.fn()
    renderHook(() => useServiceVersionChange(onChange, 'http://remote-sempre.test:33211'))
    await act(async () => undefined)
    expect(onChange).toHaveBeenCalledWith('2.0.11', undefined)
    expect(fetchMock).toHaveBeenCalledWith('http://remote-sempre.test:33211/api/v1/health', expect.anything())
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

  it('detects a changed UI identity without a service version change', async () => {
    vi.stubGlobal('fetch', vi.fn()
      .mockResolvedValueOnce(Response.json({ version: '2.0.12', ui: { id: 'first', version: '2.0.12' } }))
      .mockResolvedValueOnce(Response.json({ version: '2.0.12', ui: { id: 'second', version: '2.0.12' } })))
    const onChange = vi.fn()
    renderHook(() => useServiceVersionChange(onChange))
    await act(async () => undefined)
    expect(onChange).not.toHaveBeenCalled()
    await act(async () => vi.advanceTimersByTimeAsync(5000))
    expect(onChange).toHaveBeenCalledWith('2.0.12', { id: 'second', version: '2.0.12' })
  })

})
