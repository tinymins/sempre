import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { Management } from './Management'

describe('Management page', () => {
  let savedSettings: Record<string, unknown> | undefined

  beforeEach(() => {
    savedSettings = undefined
    localStorage.setItem('sempre.locale', 'zh-CN')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = new URL(String(input)).pathname
      if (path.endsWith('/cores')) return Response.json({ supported: [], installed: [], selected: null })
      if (path.endsWith('/network/settings')) {
        const settings = { schema: 2, revision: 1, mode: 'local', gateway_capture_host: false, automatic_switching: false, known_networks: [] }
        if (init?.method === 'PUT') savedSettings = JSON.parse(String(init.body))
        return Response.json({ settings: savedSettings ?? settings, current: { supported: true, name: 'en0', addresses: ['10.8.28.19/24'], gateway: '10.8.28.1', gateway_mac: 'aa:bb:cc:dd:ee:ff' }, platform: 'windows', gateway_available: false })
      }
      if (path.endsWith('/system')) return Response.json({ network_automation: { enabled: false, active: false, path: 'inactive' } })
      return Response.json({}, { status: 404 })
    }))
  })

  it('adds the current gateway MAC without manual entry', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Management /></SessionProvider></I18nProvider></QueryClientProvider>)

    fireEvent.click(screen.getByRole('button', { name: '模式' }))
    const add = await screen.findByRole('button', { name: '将当前网络加入' })
    await waitFor(() => expect(add).toBeEnabled())
    fireEvent.click(add)

    await waitFor(() => expect(savedSettings).toBeDefined())
    expect(savedSettings).toMatchObject({
      automatic_switching: true,
      known_networks: [{ gateway_mac: 'aa:bb:cc:dd:ee:ff', disable_proxy: true }],
    })
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('keeps an unavailable gateway reason inside the disabled option', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Management /></SessionProvider></I18nProvider></QueryClientProvider>)

    fireEvent.click(screen.getByRole('button', { name: '模式' }))
    expect(await screen.findByText('仅管理本机流量与 DNS，不加载网关配置。')).not.toHaveClass('border')
    expect(screen.queryByText('网关模式仅在 Linux 系统服务上可用。')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('combobox'))
    const listbox = await screen.findByRole('listbox')
    const gateway = within(listbox).getByText('网关模式').closest('.cursor-not-allowed')

    expect(gateway).toHaveClass('cursor-not-allowed')
    expect(within(gateway as HTMLElement).getByText('仅 Linux 系统服务可用')).toHaveClass('text-xs', 'text-[var(--text-muted)]')
  })
})
