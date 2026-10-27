import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { BrowserReport, LeakReport } from './types'
import { useLeakDetection, type LeakRunOptions } from './useLeakDetection'

const mocks = vi.hoisted(() => ({ api: vi.fn(), streamRequest: vi.fn(), runBrowserChecks: vi.fn() }))
vi.mock('../../lib/api', () => ({ api: mocks.api, streamRequest: mocks.streamRequest }))
vi.mock('./browserChecks', () => ({ runBrowserChecks: mocks.runBrowserChecks }))

const session = { baseURL: 'http://daemon', token: 't', expiresAt: '' }
const summary = { physicalEgress: 0, proxyRoute: 0, directRoute: 0, unattributed: 0, ipv4: 0, ipv6: 0 }
const snapshot = (ready: boolean, status: LeakReport['probes']['status']): LeakReport => ({
  id: 'run-1', startedAt: '', elapsedSeconds: 2, target: 'all', topology: 'local', platform: 'macos', processes: [], connections: [], dns: [], gaps: [],
  capture: { status: 'ok', detail: '', packets: [] }, ready, probes: { available: true, reason: '', status, results: [] }, probeSummary: summary, summary, completed: false,
})
const options: LeakRunOptions = { mode: 'quick', topology: 'local', clientIp: '', target: 'all', durationSeconds: 0, includeBrowser: true, expectedIpv4: '', expectedIpv6: '' }

let emit: (event: string, data: unknown) => void | boolean
let browserDone: () => void

beforeEach(() => {
  mocks.api.mockImplementation((_session: unknown, path: string) => path.endsWith('/preflight') ? Promise.resolve({ platform: 'macos', processes: [], capabilities: [], gaps: [] }) : Promise.resolve({ accepted: true }))
  mocks.streamRequest.mockImplementation((_session: unknown, _path: string, _body: unknown, onEvent: typeof emit, signal: AbortSignal) => {
    emit = onEvent
    return new Promise((_resolve, reject) => signal.addEventListener('abort', () => reject(new Error('aborted'))))
  })
  mocks.runBrowserChecks.mockImplementation((_signal: AbortSignal, _zh: boolean, _v4: string, _v6: string, onUpdate: (report: BrowserReport) => void) => new Promise((resolve) => {
    browserDone = () => { onUpdate({ running: false, completed: true } as BrowserReport); resolve(undefined) }
  }))
})
afterEach(() => vi.clearAllMocks())

describe('useLeakDetection', () => {
  it('runs quick probes automatically and finishes once probes and browser checks return', async () => {
    const { result } = renderHook(() => useLeakDetection(session, false))
    await act(async () => { await Promise.resolve() })
    act(() => result.current.start(options))
    expect(result.current.phase).toBe('preparing')
    expect(mocks.streamRequest.mock.calls[0][2]).toMatchObject({ topology: 'local', durationSeconds: 40 })

    act(() => { emit('snapshot', snapshot(true, 'idle')) })
    expect(result.current.phase).toBe('probing')
    expect(mocks.api).toHaveBeenCalledWith(session, '/network/leak-detection/observe/run-1/probes', expect.objectContaining({ method: 'POST' }))
    expect(mocks.runBrowserChecks).toHaveBeenCalledOnce()

    act(() => { emit('snapshot', snapshot(true, 'completed')) })
    expect(result.current.phase).toBe('probing')
    await act(async () => { browserDone(); await Promise.resolve() })
    expect(result.current.phase).toBe('done')
    expect(result.current.running).toBe(false)
  })

  it('observes applications without starting probes', async () => {
    const { result } = renderHook(() => useLeakDetection(session, false))
    act(() => result.current.start({ ...options, mode: 'observe', topology: 'gateway', clientIp: ' 192.168.1.9 ', durationSeconds: 60 }))
    expect(mocks.streamRequest.mock.calls[0][2]).toMatchObject({ topology: 'gateway', clientIp: '192.168.1.9', durationSeconds: 60 })
    act(() => { emit('snapshot', snapshot(true, 'idle')) })
    expect(result.current.phase).toBe('observing')
    expect(mocks.api).not.toHaveBeenCalledWith(session, expect.stringContaining('/probes'), expect.anything())
    act(() => result.current.stop())
    expect(result.current.phase).toBe('stopped')
  })

  it('checks only the browser for gateway clients', async () => {
    const { result } = renderHook(() => useLeakDetection(session, false))
    act(() => result.current.start({ ...options, topology: 'gateway' }))
    expect(mocks.streamRequest).not.toHaveBeenCalled()
    expect(result.current.phase).toBe('probing')
    await act(async () => { browserDone(); await Promise.resolve() })
    expect(result.current.phase).toBe('done')
  })
})
