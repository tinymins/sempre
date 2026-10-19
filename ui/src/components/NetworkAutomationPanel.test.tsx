import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { AcmeContentBoundary } from './AcmeContentBoundary'
import { RoutingRules } from '../pages/RoutingRules'
import { NetworkAutomationPanel } from './NetworkAutomationPanel'

describe('CN routing rule set', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  })
  afterEach(() => { cleanup(); vi.unstubAllGlobals(); sessionStorage.clear(); localStorage.clear() })

  it('persists CN routing without changing network recognition rules', async () => {
    const settings = { schema: 2, revision: 1, mode: 'local', gateway_capture_host: false, automatic_switching: true, known_networks: [{ id: 'home', name: 'Home', gateway_mac: 'aa:bb:cc:dd:ee:ff', disable_proxy: true }] }
    let saved: unknown
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init: RequestInit = {}) => {
      const response = otherResponse(String(input), 'simple')
      if (response) return response
      if (init.method === 'PUT') saved = JSON.parse(String(init.body))
      return Response.json({ settings: saved ?? settings, current: {} })
    }))
    renderPanel()
    expect(await screen.findByRole('heading', { name: 'Mainland China domains' })).toBeInTheDocument()
    const toggle = await screen.findByRole('switch', { name: 'Enable CN routing' })
    await waitFor(() => expect(toggle).toBeEnabled())
    expect(toggle).not.toBeChecked()
    expect(screen.getByRole('combobox', { name: 'CN routing mode' })).toHaveTextContent('Direct system route')
    expect(screen.queryByRole('switch', { name: 'Enable CN direct system routes' })).not.toBeInTheDocument()
    fireEvent.click(toggle)
    await waitFor(() => expect(saved).toEqual({ ...settings, cn_direct_routes: true }))
    expect(screen.getByText(/Real CN IPs use the physical uplink/)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('combobox', { name: 'CN routing mode' }))
    fireEvent.click(await screen.findByText('Proxy', { exact: true }))
    await waitFor(() => expect(saved).toEqual({ ...settings, cn_direct_routes: true, cn_routing_mode: 'proxy' }))
    expect(await screen.findByText('DNS · CN pre-routing')).toBeInTheDocument()
  })

  it('disables CN host routing in gateway mode', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => otherResponse(String(input), 'advanced') ?? Response.json({
      settings: { mode: 'gateway', cn_direct_routes: false, known_networks: [] }, current: {},
    })))
    renderPanel()
    fireEvent.click(await screen.findByRole('button', { name: /CN routing/ }))
    expect(await screen.findByRole('switch', { name: 'Enable CN routing' })).toBeDisabled()
    expect(screen.getByRole('heading', { name: 'CN routing' })).toBeInTheDocument()
    expect(screen.getByRole('combobox', { name: 'CN routing mode' })).toHaveTextContent('Direct system route')
  })
})

function renderPanel() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><AcmeContentBoundary><RoutingRules /><NetworkAutomationPanel /></AcmeContentBoundary></SessionProvider></I18nProvider></QueryClientProvider>)
}

function otherResponse(input: string, mode: string) {
  const path = new URL(input).pathname
  if (path.endsWith('/ui/settings')) return Response.json({ ui_mode: mode })
  if (path.endsWith('/system')) return Response.json({})
  if (path.endsWith('/runtime/proxies')) return Response.json([{ name: 'DNS · CN pre-routing', all: ['node-a'], now: 'node-a', type: 'Selector' }])
  if (path.endsWith('/dns/settings')) return Response.json({ settings: { enabled: true, domestic_domains: { enabled: true, mode: 'direct' }, rule_sets: [] }, status: { domestic_domain_count: 200 } })
  return null
}
