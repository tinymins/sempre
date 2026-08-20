import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { MemoryRouter } from 'react-router-dom'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
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

describe('Shell sidebar', () => {
  beforeEach(() => {
    localStorage.clear()
    sessionStorage.clear()
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify(systemStatus), { status: 200, headers: { 'Content-Type': 'application/json' } })))
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
          <MemoryRouter>
            <Shell><div>Page content</div></Shell>
          </MemoryRouter>
        </SessionProvider>
      </I18nProvider>
    </QueryClientProvider>,
  )
}
