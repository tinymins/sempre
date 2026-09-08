import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { Management as ManagementPage } from './Management'
import { ServiceUpdateFlow } from '../features/service/ServiceUpdateFlow'

function Management() {
  return <ServiceUpdateFlow><ManagementPage /></ServiceUpdateFlow>
}

describe('Management page', () => {
  let coreTask: Record<string, unknown> | null
  let coresResponse: Record<string, unknown>
  let cancelledTask = ''
  let upgradeRequested = false
  let serviceUpdateTask: Record<string, unknown> | null

  beforeEach(() => {
    coreTask = null
    coresResponse = { supported: [], installed: [], selected: null }
    cancelledTask = ''
    upgradeRequested = false
    serviceUpdateTask = null
    localStorage.setItem('sempre.locale', 'zh-CN')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = new URL(String(input)).pathname
      if (path.endsWith('/cores/download')) {
        if (init?.method === 'DELETE') {
          cancelledTask = new URL(String(input)).searchParams.get('id') || ''
          coreTask = null
        }
        return Response.json({ task: coreTask })
      }
      if (path.endsWith('/cores')) return Response.json(coresResponse)
      if (path.endsWith('/network/settings')) {
        const settings = { schema: 2, revision: 1, mode: 'local', gateway_capture_host: false, automatic_switching: false, known_networks: [] }
        return Response.json({ settings, current: { supported: true, name: 'en0', addresses: ['10.8.28.19/24'], gateway: '10.8.28.1', gateway_mac: 'aa:bb:cc:dd:ee:ff' }, platform: 'windows', gateway_available: false })
      }
      if (path.endsWith('/service/update/task')) return Response.json({ task: serviceUpdateTask })
      if (path.endsWith('/service/update')) {
        const update = { current_version: '2.0.8', latest_version: '2.1.0', update_available: true, published_at: '2026-09-07T09:15:18Z', release_notes: '## Highlights\n\n- Safer one-click upgrades.', repository: 'https://code.example/sempre' }
        if (init?.method === 'POST') {
          upgradeRequested = true
          serviceUpdateTask = { id: 'update-1', state: 'running', stage: 'downloading', current_version: '2.0.8', target_version: '2.1.0', artifact: 'sempre-bundle-windows-amd64.zip', downloaded_bytes: 50 * 1024 * 1024, total_bytes: 100 * 1024 * 1024, bytes_per_second: 5 * 1024 * 1024, eta_seconds: 10, started_at: new Date().toISOString(), updated_at: new Date().toISOString() }
          return Response.json({ task: serviceUpdateTask }, { status: 202 })
        }
        return Response.json(update)
      }
      if (path.endsWith('/system')) return Response.json({ version: '2.0.8', mode: 'system', service: 'running' })
      return Response.json({}, { status: 404 })
    }))
  })

  it('keeps automatic network switching out of management mode', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Management /></SessionProvider></I18nProvider></QueryClientProvider>)

    fireEvent.click(screen.getByRole('button', { name: '模式' }))
    expect(await screen.findByText('仅管理本机流量与 DNS，不加载网关配置。')).toBeInTheDocument()
    expect(screen.queryByText('自动网络切换')).not.toBeInTheDocument()
  })

  afterEach(() => {
    cleanup()
    sessionStorage.clear()
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

  it('separates the console from service updates and starts a verified upgrade', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Management /></SessionProvider></I18nProvider></QueryClientProvider>)

    expect(screen.queryByRole('button', { name: 'Web 与 UI' })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '控制台' }))
    expect(await screen.findByText('Web')).toBeInTheDocument()
    expect(screen.queryByText('Sempre 系统服务')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Sempre 服务' }))
    expect(await screen.findByText('2.0.8')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '检查更新' }))
    expect(await screen.findByText('2.1.0')).toBeInTheDocument()
    expect(screen.getByText('Safer one-click upgrades.', { exact: false })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '立即升级' }))

    await waitFor(() => expect(upgradeRequested).toBe(true))
    const dialog = await screen.findByRole('dialog', { name: /正在更新 Sempre/ })
    expect(within(dialog).getByText('正在下载安装包')).toBeInTheDocument()
    expect(within(dialog).getByText('50.0 MiB / 100.0 MiB')).toBeInTheDocument()
    expect(within(dialog).getByText('5.0 MiB/s')).toBeInTheDocument()
    expect(within(dialog).getByText('约 10 秒')).toBeInTheDocument()
    expect(within(dialog).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '50')
  })

  it('shows the selected core as a disabled current-use action', async () => {
    const installation = { explicit: true, digest: 'sha256:digest', source: 'release', installed_at: '2026-09-07T00:00:00Z' }
    coresResponse = {
      supported: ['sing-box'],
      selected: { core: 'sing-box', reference: 'stable' },
      installed: [
        { core: 'sing-box', repository: 'SagerNet/sing-box', reference: 'sing-box@1.13.18', official: true, version: '1.13.18', channels: ['stable'], installation },
        { core: 'sing-box', repository: 'SagerNet/sing-box', reference: 'sing-box@1.12.20', official: true, version: '1.12.20', channels: [], installation },
      ],
    }
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Management /></SessionProvider></I18nProvider></QueryClientProvider>)

    expect(await screen.findByRole('button', { name: '当前使用' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '使用' })).toBeEnabled()
  })

  it('shows byte progress and clears a cancelled download after confirmation', async () => {
    coreTask = {
      id: 'download-1', operation: 'install', reference: 'sing-box:tinymins/sing-box@1.13.15-ddns.1',
      state: 'running', stage: 'downloading', artifact: 'sing-box-darwin-arm64.tar.gz',
      downloaded_bytes: 5 * 1024 * 1024, total_bytes: 10 * 1024 * 1024,
      started_at: '2026-09-07T00:00:00Z', finished_at: null, error: null,
    }
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Management /></SessionProvider></I18nProvider></QueryClientProvider>)

    expect(await screen.findByText('5.0 MiB / 10.0 MiB · 50%')).toBeInTheDocument()
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '50')
    fireEvent.click(screen.getByRole('button', { name: '取消下载' }))

    const dialog = await screen.findByRole('dialog')
    expect(within(dialog).getByText('下载将立即终止，临时文件和任务列表项会被清除。')).toBeInTheDocument()
    fireEvent.click(within(dialog).getByRole('button', { name: '取消下载' }))

    await waitFor(() => expect(cancelledTask).toBe('download-1'))
    await waitFor(() => expect(screen.queryByText('下载任务')).not.toBeInTheDocument())
  })
})
