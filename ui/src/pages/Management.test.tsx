import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { AcmeContentBoundary } from '../components/AcmeContentBoundary'
import { Management as ManagementPage } from './Management'
import { ServiceUpdateFlow } from '../features/service/ServiceUpdateFlow'
import { clearServiceUpdateMarker } from '../lib/useServiceUpdateTask'

function Management() {
  return <AcmeContentBoundary><ServiceUpdateFlow><ManagementPage /></ServiceUpdateFlow></AcmeContentBoundary>
}

let uiMode: 'simple' | 'advanced' = 'advanced'

describe('Management page', () => {
  let coreTask: Record<string, unknown> | null
  let coresResponse: Record<string, unknown>
  let upgradeRequested = false
  let serviceUpdateTask: Record<string, unknown> | null
  let serviceUpdateConfirmed: boolean | null
  let uiUpdateConfirmed: boolean | null

  beforeEach(() => {
    clearServiceUpdateMarker()
    uiMode = 'advanced'
    coreTask = null
    coresResponse = { supported: [], installed: [], selected: null }
    upgradeRequested = false
    serviceUpdateTask = null
    serviceUpdateConfirmed = null
    uiUpdateConfirmed = null
    localStorage.setItem('sempre.locale', 'zh-CN')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = new URL(String(input)).pathname
      if (String(input).endsWith('/api/v1/ui/settings')) {
        if (init?.method === 'PUT') uiMode = JSON.parse(String(init.body)).ui_mode
        return Response.json({ ui_mode: uiMode })
      }
      if (path.endsWith('/cores/download')) {
        if (init?.method === 'DELETE') {
          coreTask = null
        }
        return Response.json({ task: coreTask })
      }
      if (path.endsWith('/cores')) return Response.json(coresResponse)
      if (path.endsWith('/cores/install') || path.endsWith('/cores/update')) return Response.json({ task: null })
      if (path.endsWith('/network/settings')) {
        const settings = { schema: 2, revision: 1, mode: 'local', gateway_capture_host: false, automatic_switching: false, known_networks: [] }
        return Response.json({ settings, current: { supported: true, name: 'en0', addresses: ['10.8.28.19/24'], gateway: '10.8.28.1', gateway_mac: 'aa:bb:cc:dd:ee:ff' }, platform: 'windows', gateway_available: false })
      }
      if (path.endsWith('/service/update/task')) return Response.json({ task: serviceUpdateTask })
      if (path.endsWith('/service/update/confirm')) {
        serviceUpdateConfirmed = JSON.parse(String(init?.body)).confirmed
        serviceUpdateTask = serviceUpdateConfirmed ? { ...serviceUpdateTask, stage: 'installing' } : { ...serviceUpdateTask, state: 'cancelled', stage: 'cancelled' }
        return Response.json({ task: serviceUpdateTask })
      }
      if (path.endsWith('/service/update/settings')) return Response.json({ settings: { schema: 1, allow_prerelease: false } })
      if (path.endsWith('/service/update')) {
        const update = { current_version: '2.0.8', latest_version: '2.1.0', update_available: true, published_at: '2026-09-07T09:15:18Z', release_notes: '## Highlights\n\n- Safer one-click upgrades.', repository: 'https://code.example/sempre' }
        if (init?.method === 'POST') {
          upgradeRequested = true
          serviceUpdateTask = { id: 'update-1', state: 'running', stage: 'downloading', current_version: '2.0.8', target_version: '', artifact: 'sempre-bundle-windows-amd64.zip', downloaded_bytes: 50 * 1024 * 1024, total_bytes: 100 * 1024 * 1024, bytes_per_second: 5 * 1024 * 1024, eta_seconds: 10, started_at: new Date().toISOString(), updated_at: new Date().toISOString() }
          return Response.json({ task: serviceUpdateTask }, { status: 202 })
        }
        return Response.json(update)
      }
      if (path.endsWith('/ui/update') || path.endsWith('/ui/install') || path.endsWith('/ui/upload')) return Response.json({ proposal: { id: 'ui-update-1', current_version: '1.0.0', target_version: path.endsWith('/ui/upload') ? '3.0.0' : '2.0.0', name: 'Sempre UI' } })
      if (path.endsWith('/ui/confirm')) { uiUpdateConfirmed = JSON.parse(String(init?.body)).confirmed; return uiUpdateConfirmed ? Response.json({}) : new Response(null, { status: 204 }) }
      if (path.endsWith('/ui')) return Response.json({ installed: true, metadata: { manifest: { name: 'Sempre UI', version: '1.0.0' }, source_type: 'official', sha256: 'abcdef0123456789' } })
      if (path.endsWith('/system')) return Response.json({ version: '2.0.8', mode: 'system', service: 'running' })
      return Response.json({}, { status: 404 })
    }))
  })

  afterEach(() => {
    cleanup()
    sessionStorage.clear()
    vi.unstubAllGlobals()
  })

  it('confirms the package-reported Sempre version only after validation', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Management /></SessionProvider></I18nProvider></QueryClientProvider>)
    fireEvent.click(screen.getByRole('button', { name: '备份与更新' }))
    fireEvent.click(await screen.findByRole('button', { name: '检查更新' }))
    fireEvent.click(await screen.findByRole('button', { name: '立即升级' }))
    await waitFor(() => expect(upgradeRequested).toBe(true))
    expect(screen.getByText('查询中')).toBeInTheDocument()

    serviceUpdateTask = { ...serviceUpdateTask, stage: 'awaiting_confirmation', target_version: '2.1.0' }
    const confirmation = await screen.findByRole('dialog', { name: '确认更新 Sempre？' })
    expect(within(confirmation).getByText('v2.0.8')).toBeInTheDocument()
    expect(within(confirmation).getByText('v2.1.0')).toBeInTheDocument()
    expect(serviceUpdateConfirmed).toBeNull()
    fireEvent.click(within(confirmation).getByRole('button', { name: '取消更新' }))
    await waitFor(() => expect(serviceUpdateConfirmed).toBe(false))
  })

  it('requires confirmation for downloaded and uploaded UI packages', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Management /></SessionProvider></I18nProvider></QueryClientProvider>)
    fireEvent.click(screen.getByRole('button', { name: '备份与更新' }))
    const updateUI = await screen.findByRole('button', { name: '更新' })
    await waitFor(() => expect(updateUI).toBeEnabled())
    fireEvent.click(updateUI)
    let confirmation = await screen.findByRole('dialog', { name: '确认更新 UI？' })
    expect(within(confirmation).getByText('v1.0.0')).toBeInTheDocument()
    expect(within(confirmation).getByText('v2.0.0')).toBeInTheDocument()
    fireEvent.click(within(confirmation).getByRole('button', { name: '取消更新' }))
    await waitFor(() => expect(uiUpdateConfirmed).toBe(false))

    uiUpdateConfirmed = null
    const upload = screen.getByLabelText('上传 ZIP')
    fireEvent.change(upload, { target: { files: [new File(['ui'], 'sempre-ui.zip', { type: 'application/zip' })] } })
    confirmation = await screen.findByRole('dialog', { name: '确认更新 UI？' })
    expect(within(confirmation).getByText('v3.0.0')).toBeInTheDocument()
    expect(uiUpdateConfirmed).toBeNull()
    fireEvent.click(within(confirmation).getByRole('button', { name: '确认安装' }))
    await waitFor(() => expect(uiUpdateConfirmed).toBe(true))
  })
})
