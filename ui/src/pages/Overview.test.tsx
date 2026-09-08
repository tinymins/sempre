import { cleanup, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { Overview } from './Overview'

vi.mock('../components/RuntimeChart', () => ({ RuntimeChart: () => <div data-testid="runtime-chart" /> }))

const configuredSystem = {
  version: '0.3.0', commit: 'test', date: '2026-09-01', mode: 'system', service: 'running', desired_state: 'running',
  service_memory: 10 * 1024 * 1024,
  runtime: { state: 'running', pid: 1234 }, selected: { core: 'sing-box', ref: 'stable' },
  active: { core: 'sing-box', ref: 'stable', version: '1.13.18', config_hash: 'abc' }, pending: false,
  network_automation: { enabled: true, active: true, path: 'direct', network_name: 'Office LAN', interface: 'en0', gateway: '10.23.0.1', gateway_mac: '10:8f:fe:6b:a0:02' },
  private_access: { profile_revision: 2, active: true, interface: 'en0', interface_addresses: ['10.8.28.19/24'], connectors: [{ tag: 'home-wg', mode: 'direct', home_networks: ['家'], matched_network: '家' }] },
  web: { listen: '127.0.0.1:33211', local_url: 'http://sempre.test', password_set: true, password_warning: false }, ui: { installed: true }, capabilities: {},
}

describe('Overview', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
    sessionStorage.clear()
  })

  it('keeps the six metrics and realtime chart for a configured running core', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const path = new URL(String(input)).pathname
      if (path.endsWith('/runtime/events')) return new Response('')
      if (path.endsWith('/runtime/overview')) return Response.json({ core: 'sing-box', version: '1.13.18', connections: 12, download: 4096, upload: 2048 })
      return Response.json(configuredSystem)
    }))
    renderOverview()

    expect(await screen.findByText('Sempre 0.3.0')).toBeInTheDocument()
    expect(screen.getByText('sing-box 1.13.18')).toBeInTheDocument()
    expect(screen.getByText('Automatic network switching')).toBeInTheDocument()
    expect(screen.getByText('Office LAN')).toBeInTheDocument()
    expect(screen.getByText('en0 · 10.23.0.1')).toBeInTheDocument()
    expect(screen.getByText('Public direct')).toBeInTheDocument()
    expect(screen.getByText('Home network auto-direct')).toBeInTheDocument()
    expect(screen.getAllByText('Direct').length).toBeGreaterThanOrEqual(2)
    expect(screen.getByText('en0 · 10.8.28.19/24')).toBeInTheDocument()
    expect(screen.getAllByText('Download')).toHaveLength(2)
    expect(screen.getAllByText('Upload')).toHaveLength(2)
    expect(screen.getByText('Active connections')).toBeInTheDocument()
    expect(screen.getByText('Memory')).toBeInTheDocument()
    expect(screen.getByTestId('runtime-chart')).toBeInTheDocument()
    expect(screen.queryByText('Smart diagnosis & configuration')).not.toBeInTheDocument()
  })

  it('shows service and managed core memory separately', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const path = new URL(String(input)).pathname
      if (path.endsWith('/runtime/events')) {
        return new Response('event: memory\ndata: {"topic":"memory","timestamp":"2026-09-02T00:00:00Z","sequence":1,"data":{"inuse":20971520}}\n\n')
      }
      if (path.endsWith('/runtime/overview')) return Response.json({ core: 'sing-box', version: '1.13.18', connections: 0, download: 0, upload: 0 })
      return Response.json(configuredSystem)
    }))
    renderOverview()

    expect(await screen.findByText('10.0 MiB + 20.0 MiB')).toBeInTheDocument()
  })

  it('hides home-network direct access when it is not applicable', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const path = new URL(String(input)).pathname
      if (path.endsWith('/runtime/events')) return new Response('')
      if (path.endsWith('/runtime/overview')) return Response.json({ core: 'sing-box', version: '1.13.18', connections: 0, download: 0, upload: 0 })
      return Response.json({ ...configuredSystem, private_access: { ...configuredSystem.private_access, connectors: [] } })
    }))
    renderOverview()

    expect(await screen.findByText('Sempre 0.3.0')).toBeInTheDocument()
    expect(screen.queryByText('Home network auto-direct')).not.toBeInTheDocument()
    expect(screen.queryByText('Not applicable')).not.toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Core Status' })).toBeInTheDocument()
  })

  it('shows a recognized network as pending until its route change is applied', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const path = new URL(String(input)).pathname
      if (path.endsWith('/runtime/events')) return new Response('')
      if (path.endsWith('/runtime/overview')) return Response.json({ core: 'sing-box', version: '1.13.18', connections: 0, download: 0, upload: 0 })
      return Response.json({
        ...configuredSystem,
        pending: true,
        network_automation: { ...configuredSystem.network_automation, path: 'unknown' },
      })
    }))
    renderOverview()

    expect(await screen.findByText('Automatic network switching')).toBeInTheDocument()
    expect(screen.getByText('Pending')).toBeInTheDocument()
    expect(screen.queryByText('Unknown')).not.toBeInTheDocument()
  })

  it('omits network automation when automatic switching is disabled', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const path = new URL(String(input)).pathname
      if (path.endsWith('/runtime/events')) return new Response('')
      if (path.endsWith('/runtime/overview')) return Response.json({ core: 'sing-box', version: '1.13.18', connections: 0, download: 0, upload: 0 })
      return Response.json({ ...configuredSystem, network_automation: { enabled: false, active: false, path: 'inactive' } })
    }))
    renderOverview()

    expect(await screen.findByText('Sempre 0.3.0')).toBeInTheDocument()
    expect(screen.queryByText('Automatic network switching')).not.toBeInTheDocument()
  })

  it('shows smart diagnosis only while initial core setup is incomplete', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => Response.json({ ...configuredSystem, runtime: { state: 'idle' }, selected: undefined, active: undefined })))
    renderOverview()

    expect(await screen.findByText('Smart diagnosis & configuration')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Core Status' })).toBeInTheDocument()
  })
})

function renderOverview() {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <I18nProvider><SessionProvider><MemoryRouter><Overview /></MemoryRouter></SessionProvider></I18nProvider>
    </QueryClientProvider>,
  )
}
