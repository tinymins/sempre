import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { MemoryRouter } from 'react-router-dom'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { ThemeProvider } from '../lib/theme'
import { Shell } from './Shell'

const systemStatus = {
  version: '0.2.0',
  commit: 'test',
  date: '2026-08-05',
  mode: 'service',
  service: 'running',
  desired_state: 'running',
  runtime: { state: 'running' },
  pending: false,
  web: { listen: '127.0.0.1:33211', local_url: 'http://sempre.test', password_set: true, password_warning: false },
  ui: { installed: true },
  capabilities: {},
}

const runtimeStatus = {
  desired_state: 'running',
  runtime_state: 'running',
  active: { core: 'sing-box', ref: 'stable', version: '1.13.18', exact_reference: 'sing-box@1.13.18', config_hash: 'a'.repeat(64) },
  pid: 1234,
  started_at: '2026-09-01T00:00:00Z',
  uptime_seconds: 60,
  restart_count: 0,
  pending: false,
  pending_changes: [],
  last_transition: '2026-09-01T00:00:00Z',
  actions: { start: { allowed: false }, stop: { allowed: true }, restart: { allowed: true } },
}

describe('Shell sidebar', () => {
  beforeEach(() => {
    localStorage.clear()
    sessionStorage.clear()
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const path = new URL(String(input)).pathname
      return Response.json(path.endsWith('/runtime/status') ? runtimeStatus : systemStatus)
    }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('collapses to an accessible icon rail and persists the choice', async () => {
    const rendered = renderShell()
    const shell = rendered.container.querySelector<HTMLElement>('[data-sidebar-collapsed]')
    expect(shell).toHaveAttribute('data-sidebar-collapsed', 'false')
    expect(shell?.style.getPropertyValue('--shell-sidebar-width')).toBe('14rem')

    const collapse = screen.getByRole('button', { name: 'Collapse sidebar' })
    expect(collapse).toHaveAttribute('aria-expanded', 'true')
    expect(screen.getByRole('link', { name: 'Subscriptions' })).not.toHaveAttribute('title')
    fireEvent.click(collapse)

    expect(shell).toHaveAttribute('data-sidebar-collapsed', 'true')
    expect(shell?.style.getPropertyValue('--shell-sidebar-width')).toBe('4rem')
    expect(localStorage.getItem('sempre.sidebar.collapsed')).toBe('true')
    expect(screen.getByRole('button', { name: 'Expand sidebar' })).toHaveAttribute('aria-expanded', 'false')
    expect(screen.getByRole('link', { name: 'Subscriptions' })).toHaveAttribute('title', 'Subscriptions')
    expect(screen.getByRole('link', { name: 'Network Test' })).toHaveAttribute('title', 'Network Test')
    expect(await screen.findByLabelText('Core: running')).toBeInTheDocument()
  })

  it('restores the saved state and can expand again', () => {
    localStorage.setItem('sempre.sidebar.collapsed', 'true')
    const rendered = renderShell()
    const shell = rendered.container.querySelector<HTMLElement>('[data-sidebar-collapsed]')

    expect(shell).toHaveAttribute('data-sidebar-collapsed', 'true')
    fireEvent.click(screen.getByRole('button', { name: 'Expand sidebar' }))

    expect(shell).toHaveAttribute('data-sidebar-collapsed', 'false')
    expect(localStorage.getItem('sempre.sidebar.collapsed')).toBe('false')
    expect(screen.getByRole('button', { name: 'Collapse sidebar' })).toHaveAttribute('aria-expanded', 'true')
  })

  it('keeps the mobile drawer workflow independent of the desktop state', () => {
    localStorage.setItem('sempre.sidebar.collapsed', 'true')
    renderShell()

    fireEvent.click(screen.getByTitle('Menu'))
    expect(screen.getByRole('button', { name: 'Close navigation' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('link', { name: 'Subscriptions' }))
    expect(screen.queryByRole('button', { name: 'Close navigation' })).not.toBeInTheDocument()
    expect(localStorage.getItem('sempre.sidebar.collapsed')).toBe('true')
  })

  it('places restart before language and marks pending changes with a red dot', async () => {
    const pendingStatus = {
      ...runtimeStatus,
      pending: true,
      pending_changes: [
        { type: 'core', previous: 'sing-box@1.12.20', current: 'sing-box@1.14.0-beta.13' },
        { type: 'configuration', fields: ['dns', 'management_api', 'transparent_proxy'] },
      ],
    }
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init: RequestInit = {}) => {
      const path = new URL(String(input)).pathname
      if (path.endsWith('/runtime/restart') && init.method === 'POST') {
        return Response.json({ action: 'restart', status: pendingStatus }, { status: 202 })
      }
      return Response.json(path.endsWith('/runtime/status') ? pendingStatus : systemStatus)
    }))
    renderShell()

    const restart = await screen.findByRole('button', { name: 'Restart core' })
    const language = screen.getByTitle('Language')
    expect(restart).toHaveAttribute('title', 'Restart core')
    expect(restart.parentElement?.nextElementSibling).toBe(language)
    await waitFor(() => expect(restart.parentElement?.querySelector('[data-restart-required]')).toHaveClass('bg-red-500'))

    fireEvent.click(restart)
    const dialog = screen.getByRole('dialog', { name: 'Restart the core?' })
    expect(within(dialog).getByText('Changes to apply')).toBeInTheDocument()
    expect(within(dialog).getByText('Core switch')).toBeInTheDocument()
    expect(within(dialog).getByText('DNS configuration, Management API, and Transparent proxy')).toBeInTheDocument()
    expect(fetch).not.toHaveBeenCalledWith('http://sempre.test/api/v1/runtime/restart', expect.objectContaining({ method: 'POST' }))

    fireEvent.click(within(dialog).getByRole('button', { name: 'Restart core' }))
    await waitFor(() => expect(fetch).toHaveBeenCalledWith('http://sempre.test/api/v1/runtime/restart', expect.objectContaining({ method: 'POST' })))
    expect(await screen.findByRole('status')).toHaveTextContent('Operation accepted')
  })

  it('shows restart failures from the global control', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init: RequestInit = {}) => {
      const path = new URL(String(input)).pathname
      if (path.endsWith('/runtime/restart') && init.method === 'POST') {
        return Response.json({ error: { code: 'RUNTIME_ERROR', message: 'Managed core is unavailable' } }, { status: 503 })
      }
      return Response.json(path.endsWith('/runtime/status') ? runtimeStatus : systemStatus)
    }))
    renderShell()

    fireEvent.click(await screen.findByRole('button', { name: 'Restart core' }))
    fireEvent.click(within(screen.getByRole('dialog', { name: 'Restart the core?' })).getByRole('button', { name: 'Restart core' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Managed core is unavailable')
  })

  it('replaces the accepted restart notice with an asynchronous rollback result', async () => {
    const failed = { ...runtimeStatus.active, config_hash: 'b'.repeat(64) }
    const finalStatus = {
      ...runtimeStatus,
      last_exit: 'exit status 1',
      last_failure: { stage: 'startup failed for sing-box@1.13.18', error: 'exit status 1', occurred_at: '2026-09-01T00:01:00Z', failed, rolled_back_to: runtimeStatus.active },
    }
    let statusReads = 0
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init: RequestInit = {}) => {
      const path = new URL(String(input)).pathname
      if (path.endsWith('/runtime/restart') && init.method === 'POST') {
        return Response.json({ action: 'restart', status: { ...runtimeStatus, runtime_state: 'stopping', active: failed, pending: true } }, { status: 202 })
      }
      if (path.endsWith('/runtime/status')) {
        statusReads += 1
        return Response.json(statusReads > 1 ? finalStatus : runtimeStatus)
      }
      return Response.json(systemStatus)
    }))
    renderShell()

    fireEvent.click(await screen.findByRole('button', { name: 'Restart core' }))
    fireEvent.click(within(screen.getByRole('dialog', { name: 'Restart the core?' })).getByRole('button', { name: 'Restart core' }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Managed core startup failed. The last working configuration was restored.')
    expect(alert).toHaveTextContent('Error: exit status 1')
    expect(alert).toHaveTextContent('Rollback: sing-box@1.13.18 · bbbbbbbb...bbbbbb → sing-box@1.13.18 · aaaaaaaa...aaaaaa')
  })

  it('dismisses the password warning only for the current shell mount', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      ...systemStatus,
      web: { ...systemStatus.web, password_set: false, password_warning: true },
    }), { status: 200, headers: { 'Content-Type': 'application/json' } })))
    const rendered = renderShell()

    const warning = await screen.findByText('The administrator password is empty. Set one as soon as possible.')
    fireEvent.click(within(warning).getByRole('button', { name: 'Close' }))
    expect(screen.queryByText('The administrator password is empty. Set one as soon as possible.')).not.toBeInTheDocument()

    rendered.unmount()
    renderShell()
    expect(await screen.findByText('The administrator password is empty. Set one as soon as possible.')).toBeInTheDocument()
  })
})

function renderShell() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(
    <QueryClientProvider client={client}>
      <I18nProvider>
        <SessionProvider>
          <ThemeProvider>
            <MemoryRouter>
              <Shell><div>Page content</div></Shell>
            </MemoryRouter>
          </ThemeProvider>
        </SessionProvider>
      </I18nProvider>
    </QueryClientProvider>,
  )
}
