import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { NetworkAutomation } from './NetworkAutomation'

describe('Network automation page', () => {
  let savedSettings: Record<string, unknown> | undefined
  let networkAutomation: { enabled: boolean; active: boolean; path: string; gateway_mac?: string }

  beforeEach(() => {
    savedSettings = undefined
    networkAutomation = { enabled: false, active: false, path: 'inactive' }
    localStorage.setItem('sempre.locale', 'zh-CN')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = new URL(String(input)).pathname
      if (path.endsWith('/network/settings')) {
        const settings = { schema: 2, revision: 1, mode: 'local', gateway_capture_host: false, automatic_switching: false, known_networks: [] }
        if (init?.method === 'PUT') savedSettings = JSON.parse(String(init.body))
        return Response.json({ settings: savedSettings ?? settings, current: { supported: true, name: 'en0', addresses: ['10.8.28.19/24'], gateway: '10.8.28.1', gateway_mac: 'aa:bb:cc:dd:ee:ff' }, platform: 'windows', gateway_available: false })
      }
      if (path.endsWith('/system')) return Response.json({ version: '2.0.8', mode: 'system', service: 'running', network_automation: networkAutomation })
      return Response.json({}, { status: 404 })
    }))
  })

  afterEach(() => {
    cleanup()
    sessionStorage.clear()
    vi.unstubAllGlobals()
  })

  it('adds the current gateway MAC without manual entry', async () => {
    renderPage()

    expect(screen.getByRole('heading', { name: '自动网络切换' })).toBeInTheDocument()
    const add = await screen.findByRole('button', { name: '将当前网络加入' })
    await waitFor(() => expect(add).toBeEnabled())
    fireEvent.click(add)

    await waitFor(() => expect(savedSettings).toBeDefined())
    expect(savedSettings).toMatchObject({
      automatic_switching: true,
      known_networks: [{ gateway_mac: 'aa:bb:cc:dd:ee:ff', disable_proxy: true }],
    })
  })

  it.each([
    [false, 'inactive', '自动切换未开启'],
    [true, 'inactive', '核心未运行'],
    [true, 'direct', '公网直连'],
    [true, 'proxy', '公网代理'],
    [true, 'unknown', '无法判定'],
  ])('labels network automation with enabled=%s and path=%s', async (enabled, path, label) => {
    networkAutomation = { enabled, active: path !== 'inactive', path }
    renderPage()

    expect(await screen.findByText(label)).toBeInTheDocument()
    if (!enabled) expect(screen.queryByText('核心未运行')).not.toBeInTheDocument()
  })

  it('shows a recognized staged network as pending', async () => {
    networkAutomation = { enabled: true, active: true, path: 'unknown', gateway_mac: 'aa:bb:cc:dd:ee:ff' }
    renderPage()

    expect(await screen.findByText('待应用')).toBeInTheDocument()
    expect(screen.queryByText('无法判定')).not.toBeInTheDocument()
  })
})

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><NetworkAutomation /></SessionProvider></I18nProvider></QueryClientProvider>)
}
