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
  last_transition: '2026-08-03T10:00:10Z',
  actions: {
    start: { allowed: false, reason: 'managed core is already running' },
    stop: { allowed: true },
    restart: { allowed: true },
  },
}

const retryableFailureStatus: ManagedRuntimeStatus = {
  ...runningStatus,
  runtime_state: 'failed',
  active: null,
  target: runningStatus.active!,
  pid: 0,
  started_at: null,
  uptime_seconds: 0,
  last_error: 'startup failed: exit status 1',
  actions: {
    start: { allowed: true },
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

  it('keeps all lifecycle actions available after a retryable startup failure', async () => {
    const fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url.endsWith('/api/v1/runtime/restart') && init?.method === 'POST') {
        return new Response(JSON.stringify({
          action: 'restart',
          status: { ...retryableFailureStatus, runtime_state: 'restarting', active: retryableFailureStatus.target, pending: true },
        }), { status: 202, headers: { 'Content-Type': 'application/json' } })
      }
      return new Response(JSON.stringify(retryableFailureStatus), { status: 200, headers: { 'Content-Type': 'application/json' } })
    })
    vi.stubGlobal('fetch', fetch)
    renderRuntimePanel()

    const start = await screen.findByRole('button', { name: 'Start managed core' })
    await waitFor(() => expect(start).toBeEnabled())
    expect(screen.getByRole('button', { name: 'Stop managed core' })).toBeEnabled()
    const restart = screen.getByRole('button', { name: 'Restart managed core' })
    expect(restart).toBeEnabled()
    fireEvent.click(restart)

    await waitFor(() => expect(fetch).toHaveBeenCalledWith('http://sempre.test/api/v1/runtime/restart', expect.objectContaining({ method: 'POST' })))
  })

  it('describes a starting pending deployment as health-checking', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      ...runningStatus,
      runtime_state: 'starting',
      pending: true,
      actions: {
        start: { allowed: false, reason: 'managed core is starting' },
        stop: { allowed: true },
        restart: { allowed: false, reason: 'managed core is starting' },
      },
    }), { status: 200, headers: { 'Content-Type': 'application/json' } })))
    renderRuntimePanel()

    expect(await screen.findByText('The new core or configuration is being health-checked and will be committed after about 10 seconds.')).toBeInTheDocument()
  })

  it('keeps the generic pending message outside startup health checks', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      ...runningStatus,
      runtime_state: 'restarting',
      pending: true,
    }), { status: 200, headers: { 'Content-Type': 'application/json' } })))
    renderRuntimePanel()

    expect(await screen.findByText('A core or configuration change is pending and will be committed after the core runs successfully.')).toBeInTheDocument()
  })
})

describe('ConfirmDialog', () => {
  it('animates closed with stable content and resets acknowledgement before reopening', async () => {
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
    expect(screen.getByRole('dialog', { name: 'Stop Sempre Service?' })).toBeInTheDocument()
    await waitFor(() => expect(dialog).toHaveClass('opacity-0'))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())

    rendered.rerender(<ConfirmDialog open title="Stop Sempre Service?" detail="The page will disconnect." acknowledgement="I must start it on the host." confirmLabel="Stop service" cancelLabel="Cancel" onCancel={() => undefined} onConfirm={confirm} />)
    const reopened = await screen.findByRole('dialog', { name: 'Stop Sempre Service?' })
    expect(within(reopened).getByRole('checkbox')).not.toBeChecked()
    expect(within(reopened).getByRole('button', { name: 'Stop service' })).toBeDisabled()
  })
})

function renderRuntimePanel() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  return render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><MemoryRouter><RuntimeControlPanel /></MemoryRouter></SessionProvider></I18nProvider></QueryClientProvider>)
}
