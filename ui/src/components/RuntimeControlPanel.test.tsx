import { ToastProvider } from '@acme/components'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { MemoryRouter } from 'react-router-dom'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import type { ManagedRuntimeStatus } from '../lib/types'
import { RuntimeControlPanel } from './RuntimeControlPanel'
import { ConfirmDialog } from './ui'

const runningStatus: ManagedRuntimeStatus = {
  desired_state: 'running',
  runtime_state: 'running',
  active: {
    core: 'sing-box',
    ref: 'stable',
    version: '1.2.3',
    exact_reference: 'sing-box@1.2.3',
    config_hash: 'a'.repeat(64),
  },
  pid: 1234,
  started_at: '2026-08-03T10:00:00Z',
  uptime_seconds: 90,
  restart_count: 0,
  pending: false,
  private_access: { profile_revision: 2, active: true, interface: 'en0', interface_addresses: ['10.8.28.19/24'], connectors: [{ tag: 'home-wg', mode: 'direct', home_networks: ['家'], matched_network: '家' }] },
  network_automation: { enabled: true, active: true, path: 'direct', network_name: '家', interface: 'en0', gateway: '10.8.28.1', gateway_mac: 'aa:bb:cc:dd:ee:ff' },
  last_transition: '2026-08-03T10:00:10Z',
  actions: {
    start: { allowed: false, reason: 'managed core is already running' },
    stop: { allowed: true },
    restart: { allowed: true },
  },
}

describe('RuntimeControlPanel', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('confirms a managed-core stop and applies the accepted status', async () => {
    let current = runningStatus
    const fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url.endsWith('/ui/settings')) return Response.json({ ui_mode: 'advanced' })
      if (url.endsWith('/api/v1/runtime/stop') && init?.method === 'POST') {
        current = {
          ...runningStatus,
          desired_state: 'stopped',
          runtime_state: 'stopped',
          pid: 0,
          actions: {
            start: { allowed: true },
            stop: { allowed: false, reason: 'managed core is already stopped' },
            restart: { allowed: true },
          },
        }
        return new Response(JSON.stringify({ action: 'stop', status: current }), { status: 202, headers: { 'Content-Type': 'application/json' } })
      }
      return new Response(JSON.stringify(current), { status: 200, headers: { 'Content-Type': 'application/json' } })
    })
    vi.stubGlobal('fetch', fetch)
    renderRuntimePanel()

    const stop = await screen.findByRole('button', { name: 'Stop managed core' })
    await waitFor(() => expect(stop).toBeEnabled())
    fireEvent.click(stop)
    const dialog = screen.getByRole('dialog')
	expect(within(dialog).getByText(/stopping sing-box/i)).toBeInTheDocument()
    expect(within(dialog).getByText(/interrupts current proxy traffic/i)).toBeInTheDocument()
    fireEvent.click(within(dialog).getByRole('button', { name: 'Stop managed core' }))

    await waitFor(() => expect(fetch).toHaveBeenCalledWith('http://sempre.test/api/v1/runtime/stop', expect.objectContaining({ method: 'POST' })))
    await waitFor(() => expect(screen.getAllByText('Stopped').length).toBeGreaterThan(0))
  })
})

describe('ConfirmDialog', () => {
  it('resets acknowledgement before reopening', async () => {
    const confirm = vi.fn()
    const rendered = render(<ConfirmDialog open title="Stop Sempre Service?" detail="The page will disconnect." acknowledgement="I must start it on the host." confirmLabel="Stop service" cancelLabel="Cancel" onCancel={() => undefined} onConfirm={confirm} />)
    const dialog = screen.getByRole('dialog')
    const button = within(dialog).getByRole('button', { name: 'Stop service' })
    expect(button).toBeDisabled()
    fireEvent.click(within(dialog).getByRole('checkbox'))
    expect(button).toBeEnabled()
    fireEvent.click(button)
    expect(confirm).toHaveBeenCalledOnce()

    rendered.rerender(<ConfirmDialog open={false} title="Stop Sempre Service?" detail="The page will disconnect." acknowledgement="I must start it on the host." confirmLabel="Stop service" cancelLabel="Cancel" onCancel={() => undefined} onConfirm={confirm} />)

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())

    rendered.rerender(<ConfirmDialog open title="Stop Sempre Service?" detail="The page will disconnect." acknowledgement="I must start it on the host." confirmLabel="Stop service" cancelLabel="Cancel" onCancel={() => undefined} onConfirm={confirm} />)
    const reopened = await screen.findByRole('dialog', { name: 'Stop Sempre Service?' })
    expect(within(reopened).getByRole('checkbox')).not.toBeChecked()
    expect(within(reopened).getByRole('button', { name: 'Stop service' })).toBeDisabled()
  })
})

function renderRuntimePanel() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  return render(<QueryClientProvider client={client}><ToastProvider><I18nProvider><SessionProvider><MemoryRouter><RuntimeControlPanel /></MemoryRouter></SessionProvider></I18nProvider></ToastProvider></QueryClientProvider>)
}
