import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { NetworkDiagnostics } from './NetworkDiagnostics'

const conflictReport = {
  checked_at: '2026-09-28T03:24:00Z',
  target: 'https://www.google.com/generate_204',
  host: 'www.google.com',
  port: 443,
  status: 'failed',
  layers: [
    { id: 'runtime', status: 'passed', summary: 'Managed core is running' },
    { id: 'dns', status: 'passed', summary: 'Resolved 1 address(es)', evidence: ['198.18.0.67 (FakeIP)'] },
    { id: 'route', status: 'failed', summary: 'Managed FakeIP range is split across multiple routes', evidence: ['198.18.0.67 → utun4 via 10.251.1.1', 'FakeIP sample: 198.19.0.1 → utun5 via 172.19.0.1'] },
    { id: 'tcp', status: 'passed', summary: 'tcp completed', evidence: ['198.18.0.67:443: connected'] },
    { id: 'tls', status: 'failed', summary: 'tls failed', evidence: ['198.18.0.67:443: handshake timed out'] },
    { id: 'http', status: 'skipped', summary: 'TLS did not complete' },
  ],
  findings: [{
    code: 'fake_ip_route_conflict',
    severity: 'error',
    title: 'Multiple TUN routes claim the FakeIP range',
    detail: '198.18.0.1 → utun4 via 10.251.1.1; 198.19.0.1 → utun5 via 172.19.0.1',
    solutions: [
      'Do not run two transparent proxy or SASE clients with overlapping FakeIP ranges',
      'If both must remain active, configure a non-overlapping FakeIP range and matching route',
      'Avoid deleting managed routes manually because the owning client may restore them',
    ],
  }],
}

function diagnosticResponse() {
  const events = conflictReport.layers.map(layer => `event: layer-completed\ndata: ${JSON.stringify({ layer })}\n\n`)
  events.push(`event: result\ndata: ${JSON.stringify(conflictReport)}\n\n`)
  return new Response(events.join(''), { headers: { 'Content-Type': 'text/event-stream' } })
}

describe('NetworkDiagnostics', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
    sessionStorage.clear()
  })

  it('locates a FakeIP route conflict and presents actionable evidence', async () => {
    const fetch = vi.fn(async () => diagnosticResponse())
    vi.stubGlobal('fetch', fetch)
    renderPage()
    fireEvent.click(screen.getByRole('button', { name: 'Run diagnostics' }))

    expect(await screen.findByText('Issue located at the route / TUN layer', {}, { timeout: 3000 })).toBeInTheDocument()
    expect(screen.getByText('Multiple TUN routes claim the FakeIP range')).toBeInTheDocument()
    expect(screen.getByText('198.18.0.67 → utun4 via 10.251.1.1')).toBeInTheDocument()
    expect(screen.getByText(/non-overlapping FakeIP range/)).toBeInTheDocument()
    expect(fetch).toHaveBeenCalledWith('http://sempre.test/api/v1/network/diagnostics', expect.objectContaining({
      method: 'POST',
      body: JSON.stringify({ target: 'https://www.google.com/generate_204' }),
    }))
  })

  it('runs again for a user-supplied target', async () => {
    const fetch = vi.fn(async () => diagnosticResponse())
    vi.stubGlobal('fetch', fetch)
    renderPage()
    fireEvent.click(screen.getByRole('button', { name: 'Run diagnostics' }))
    await screen.findByText('Problems and solutions', {}, { timeout: 3000 })

    fireEvent.change(screen.getByLabelText('Diagnostic target'), { target: { value: 'https://example.com/' } })
    fireEvent.click(screen.getByRole('button', { name: 'Run again' }))
    await waitFor(() => expect(fetch).toHaveBeenCalledTimes(2))
    expect(fetch).toHaveBeenLastCalledWith('http://sempre.test/api/v1/network/diagnostics', expect.objectContaining({
      body: JSON.stringify({ target: 'https://example.com/' }),
    }))
  })
})

function renderPage() {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <I18nProvider>
        <SessionProvider>
          <NetworkDiagnostics />
        </SessionProvider>
      </I18nProvider>
    </QueryClientProvider>,
  )
}
