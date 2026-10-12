import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { AcmeContentBoundary } from './AcmeContentBoundary'
import { NetworkAutomationPanel } from './NetworkAutomationPanel'

describe('CN direct system routes', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  })
  afterEach(() => { cleanup(); vi.unstubAllGlobals(); sessionStorage.clear(); localStorage.clear() })

  it('persists CN routing without changing network recognition rules', async () => {
    const settings = { schema: 2, revision: 1, mode: 'local', gateway_capture_host: false, automatic_switching: true, known_networks: [{ id: 'home', name: 'Home', gateway_mac: 'aa:bb:cc:dd:ee:ff', disable_proxy: true }] }
    let saved: unknown
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init: RequestInit = {}) => {
      if (new URL(String(input)).pathname.endsWith('/system')) return Response.json({})
      if (init.method === 'PUT') saved = JSON.parse(String(init.body))
      return Response.json({ settings: saved ?? settings, current: {} })
    }))
    renderPanel()
    const toggle = await screen.findByRole('switch', { name: 'Enable CN direct system routes' })
    await waitFor(() => expect(toggle).toBeEnabled())
    fireEvent.click(toggle)
    await waitFor(() => expect(saved).toEqual({ ...settings, cn_direct_routes: true }))
    expect(screen.getByText(/Uses real-IP DNS/)).toBeInTheDocument()
  })

  it('disables CN host routing in gateway mode', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => Response.json(new URL(String(input)).pathname.endsWith('/system') ? {} : {
      settings: { mode: 'gateway', cn_direct_routes: false, known_networks: [] }, current: {},
    })))
    renderPanel()
    expect(await screen.findByRole('switch', { name: 'Enable CN direct system routes' })).toBeDisabled()
  })
})

function renderPanel() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><AcmeContentBoundary><NetworkAutomationPanel /></AcmeContentBoundary></SessionProvider></I18nProvider></QueryClientProvider>)
}
