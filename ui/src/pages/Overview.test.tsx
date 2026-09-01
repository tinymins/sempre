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
