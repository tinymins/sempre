import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { NetworkTest } from './NetworkTest'
import { NodeTest } from './NodeTest'

const report = {
  checked_at: '2026-08-07T00:00:00Z',
  results: [
    { id: 'domestic-ip', name: 'Domestic IP', region: 'domestic', category: 'ip', url: 'https://ip.3322.net', ok: true, latency_ms: 38, http_status: 200, ip: '183.131.177.101', ip_metadata: { country: 'China', region: 'Zhejiang', city: 'Hangzhou', isp: 'China Telecom', asn: 4134 }, dns_answers: [{ address: '223.5.5.5', fake_ip: false }] },
    { id: 'foreign-ip', name: 'Foreign IP', region: 'foreign', category: 'ip', url: 'https://api64.ipify.org?format=json', ok: true, latency_ms: 128, http_status: 200, ip: '144.34.229.119', ip_metadata: { country: 'United States', region: 'California', city: 'Los Angeles', asn_organization: 'Cloudflare, Inc.', asn: 13335 }, dns_answers: [{ address: '198.18.0.2', fake_ip: true }] },
    { id: 'baidu', name: 'Baidu', region: 'domestic', category: 'reachability', url: 'https://www.baidu.com/', ok: true, latency_ms: 42, http_status: 200, dns_answers: [{ address: '110.242.68.66', fake_ip: false }] },
    { id: 'google', name: 'Google', region: 'foreign', category: 'reachability', url: 'https://www.google.com/generate_204', ok: false, latency_ms: 8000, http_status: 0, detail: 'context deadline exceeded', dns_answers: [{ address: '198.18.0.8', fake_ip: true }] },
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
    expect(screen.getByText('Request duration')).toBeInTheDocument()
    expect(screen.getByText('Local DNS')).toBeInTheDocument()
    expect(screen.getByText('Average request duration')).toBeInTheDocument()
    expect(screen.getAllByText('Loading...')).toHaveLength(4)
    expect(await screen.findAllByText('183.131.177.101')).toHaveLength(2)
    expect(screen.getAllByText('144.34.229.119')).toHaveLength(2)
    expect(screen.getAllByText('China · Zhejiang · Hangzhou · China Telecom · AS4134')).toHaveLength(2)
    expect(screen.getAllByText('United States · California · Los Angeles · Cloudflare, Inc. · AS13335')).toHaveLength(2)
    expect(screen.getAllByRole('row').slice(1).map((row) => row.textContent)).toEqual([
      expect.stringContaining('Domestic IP'),
      expect.stringContaining('Foreign IP'),
      expect.stringContaining('Baidu'),
      expect.stringContaining('Google'),
    ])
    expect(screen.getByText('context deadline exceeded')).toBeInTheDocument()
    expect(within(screen.getByRole('row', { name: /Baidu/ })).getByText('110.242.68.66')).toBeInTheDocument()
    expect(within(screen.getByRole('row', { name: /Baidu/ })).getByText('Real-IP')).toBeInTheDocument()
    expect(within(screen.getByRole('row', { name: /Google/ })).getByText('198.18.0.8')).toBeInTheDocument()
    expect(within(screen.getByRole('row', { name: /Google/ })).getByText('Fake-IP')).toBeInTheDocument()
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

  it('tests node latency and renders structured traffic diagnostics', async () => {
    let resolveLatency: ((value: Response) => void) | undefined
    const fetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.endsWith('/runtime/nodes')) return Response.json([{ name: 'Tokyo 01', type: 'Shadowsocks' }])
      if (url.endsWith('/runtime/nodes/delay')) {
        return new Promise<Response>((resolve) => { resolveLatency = resolve })
      }
      if (url.endsWith('/runtime/nodes/debug')) {
        return sseResponse([
          ['step', { id: 'prepare', label: '启动隔离 Core', state: 'succeeded', duration_ms: 31, data: { node: 'Tokyo 01' } }],
          ['step', { id: 'probe', label: '节点探活', state: 'succeeded', duration_ms: 82, data: { url: 'https://cp.cloudflare.com/generate_204', status: 204, bytes: 0 } }],
          ['step', { id: 'dns-baidu', label: 'DNS', state: 'succeeded', duration_ms: 44, data: { domain: 'www.baidu.com', resolver: 'dns.google', status: 0, answers: ['110.242.68.66'] } }],
          ['step', { id: 'http-baidu', label: 'HTTP', state: 'succeeded', duration_ms: 96, data: { url: 'https://www.baidu.com/', status: 200, bytes: 1536 } }],
          ['done', { node: 'Tokyo 01', duration_ms: 300 }],
        ])
      }
      throw new Error(`unexpected request ${url}`)
    })
    vi.stubGlobal('fetch', fetch)
    renderNodeTest()

    expect(screen.getByRole('heading', { name: 'Node Test' })).toBeInTheDocument()
    expect(await screen.findByText('Tokyo 01')).toBeInTheDocument()
    const latencyButton = screen.getByRole('button', { name: 'Test latency' })
    fireEvent.click(latencyButton)
    expect(latencyButton).toBeDisabled()
    resolveLatency?.(Response.json({ delay: 86 }))
    expect(await screen.findByText('86 ms')).toHaveClass('!text-emerald-600')

    fireEvent.click(screen.getByRole('button', { name: 'Debug Tokyo 01' }))
    expect(await screen.findByText('Node diagnostics · Tokyo 01')).toBeInTheDocument()
    expect(await screen.findByText('110.242.68.66')).toBeInTheDocument()
    expect(screen.getByText(/HTTP 200 · 1.5 KiB/)).toBeInTheDocument()
    expect(screen.getByText('Completed')).toBeInTheDocument()
  })
})

function sseResponse(events: Array<[string, object]>) {
  const body = events.map(([event, data]) => `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`).join('')
  return new Response(body, { headers: { 'Content-Type': 'text/event-stream' } })
}

function renderNodeTest() {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <I18nProvider>
        <SessionProvider>
          <NodeTest />
        </SessionProvider>
      </I18nProvider>
    </QueryClientProvider>,
  )
}

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
