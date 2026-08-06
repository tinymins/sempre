import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { NetworkTest } from './NetworkTest'

const report = {
  checked_at: '2026-08-07T00:00:00Z',
  results: [
    { id: 'baidu', name: 'Baidu', region: 'domestic', category: 'reachability', url: 'https://www.baidu.com/', ok: true, latency_ms: 42, http_status: 200 },
    { id: 'google', name: 'Google', region: 'foreign', category: 'reachability', url: 'https://www.google.com/generate_204', ok: false, latency_ms: 8000, http_status: 0, detail: 'context deadline exceeded' },
    { id: 'domestic-ip', name: 'Domestic IP', region: 'domestic', category: 'ip', url: 'https://ip.3322.net', ok: true, latency_ms: 38, http_status: 200, ip: '183.131.177.101' },
    { id: 'foreign-ip', name: 'Foreign IP', region: 'foreign', category: 'ip', url: 'https://api64.ipify.org?format=json', ok: true, latency_ms: 128, http_status: 200, ip: '144.34.229.119' },
  ],
}

describe('NetworkTest', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
    sessionStorage.clear()
  })

  it('runs host-side network tests on mount and refresh', async () => {
    const fetch = vi.fn()
      .mockResolvedValueOnce(Response.json(report))
      .mockImplementationOnce(() => new Promise<Response>((resolve) => {
        setTimeout(() => resolve(Response.json(report)), 50)
      }))
    vi.stubGlobal('fetch', fetch)
    renderNetworkTest()

    expect(screen.getByText('Baidu')).toBeInTheDocument()
    expect(screen.getByText('Google')).toBeInTheDocument()
    expect(screen.getAllByText('Loading...')).toHaveLength(4)
    expect(await screen.findAllByText('183.131.177.101')).toHaveLength(2)
    expect(screen.getAllByText('144.34.229.119')).toHaveLength(2)
    expect(screen.getByText('context deadline exceeded')).toBeInTheDocument()
    expect(fetch).toHaveBeenCalledWith('http://sempre.test/api/v1/network/test', expect.objectContaining({ method: 'POST' }))

    fireEvent.click(screen.getByRole('button', { name: /Refresh/ }))
    expect(screen.getByText('Google')).toBeInTheDocument()
    await waitFor(() => expect(screen.getAllByText('Loading...')).toHaveLength(4))
    expect(fetch).toHaveBeenCalledTimes(2)
  })

  it('keeps the fixed table visible when the request fails', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => Response.json({ error: { code: 'BROKEN', message: 'network test failed' } }, { status: 500 })))
    renderNetworkTest()

    expect(screen.getByText('Baidu')).toBeInTheDocument()
    expect(screen.getByText('Google')).toBeInTheDocument()
    expect(await screen.findAllByText('network test failed')).toHaveLength(4)
  })
})

function renderNetworkTest() {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <I18nProvider>
        <SessionProvider>
          <NetworkTest />
        </SessionProvider>
      </I18nProvider>
    </QueryClientProvider>,
  )
}
