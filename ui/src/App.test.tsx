import { StrictMode } from 'react'
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { App, AppContent } from './App'
import { I18nProvider } from './lib/i18n'
import { SessionProvider } from './lib/session'

vi.mock('./components/RuntimeChart', () => ({ RuntimeChart: () => <div data-testid="runtime-chart" /> }))

const systemStatus = {
  version: '0.2.0',
  commit: 'test',
  date: '2026-08-05',
  mode: 'service',
  service: 'running',
  desired_state: 'running',
  runtime: { state: 'failed' },
  pending: false,
  web: { listen: '127.0.0.1:33211', local_url: 'http://sempre.test', password_set: false, password_warning: true },
  ui: { installed: true },
  capabilities: {},
}

const runtimeStatus = {
  desired_state: 'running',
  runtime_state: 'failed',
  pid: 0,
  uptime_seconds: 0,
  restart_count: 1,
  pending: false,
  last_error: 'exit status 1',
  actions: {
    start: { allowed: true },
    stop: { allowed: true },
    restart: { allowed: true },
  },
}

describe('App', () => {
  beforeEach(() => {
    sessionStorage.clear()
    localStorage.removeItem('sempre.theme')
    localStorage.setItem('sempre.locale', 'en')
    window.location.hash = ''
    document.documentElement.classList.remove('dark')
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('starts with the actual login workflow', () => {
    render(<QueryClientProvider client={new QueryClient()}><I18nProvider><SessionProvider><App /></SessionProvider></I18nProvider></QueryClientProvider>)
    expect(screen.getByRole('heading', { name: 'Sempre' })).toBeInTheDocument()
    expect(screen.getByLabelText('Sempre address')).toHaveValue(window.location.origin)
    expect(screen.getByRole('button', { name: /Connect/ })).toBeInTheDocument()
  })

  it('starts with multi-user authentication in a server build', async () => {
    render(<QueryClientProvider client={new QueryClient()}><I18nProvider><SessionProvider><AppContent serverMode /></SessionProvider></I18nProvider></QueryClientProvider>)

    expect(await screen.findByRole('heading', { name: 'Sempre Server' })).toBeInTheDocument()
    expect(screen.getByLabelText('Email')).toBeInTheDocument()
    expect(screen.getByLabelText('Password')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeInTheDocument()
  })

  it('follows the system dark theme before authentication', async () => {
    let systemDark = true
    let onChange: (() => void) | undefined
    vi.stubGlobal('matchMedia', vi.fn((query: string): MediaQueryList => ({
      get matches() { return query === '(prefers-color-scheme: dark)' && systemDark },
      media: query,
      onchange: null,
      addListener: () => undefined,
      removeListener: () => undefined,
      addEventListener: (_type: string, listener: EventListenerOrEventListenerObject) => {
        onChange = () => typeof listener === 'function' ? listener(new Event('change')) : listener.handleEvent(new Event('change'))
      },
      removeEventListener: () => undefined,
      dispatchEvent: () => false,
    })))

    render(<QueryClientProvider client={new QueryClient()}><I18nProvider><SessionProvider><App /></SessionProvider></I18nProvider></QueryClientProvider>)

    await waitFor(() => expect(document.documentElement).toHaveClass('dark'))
    expect(screen.getByRole('heading', { name: 'Sempre' })).toBeInTheDocument()

    systemDark = false
    act(() => onChange?.())
    expect(document.documentElement).not.toHaveClass('dark')
  })

  it('enters the lazy shell after login under StrictMode when the core is unavailable', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const path = new URL(String(input)).pathname
      if (path === '/api/v1/auth/login') {
        return Response.json({ token: 'session', expires_at: '2099-01-01T00:00:00Z', warning: 'PASSWORD_EMPTY' })
      }
      if (path === '/api/v1/system') return Response.json(systemStatus)
      if (path === '/api/v1/runtime/status') return Response.json(runtimeStatus)
      if (path === '/api/v1/network/test') return Response.json({ checked_at: '2026-08-07T00:00:00Z', results: [] })
      return Response.json({ error: { code: 'NOT_FOUND', message: path } }, { status: 404 })
    }))

    render(
      <StrictMode>
        <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
          <I18nProvider><SessionProvider><App /></SessionProvider></I18nProvider>
        </QueryClientProvider>
      </StrictMode>,
    )

    fireEvent.click(screen.getByRole('button', { name: /Connect/ }))

    expect(await screen.findByRole('link', { name: 'Overview' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Network Test' })).toBeInTheDocument()
    expect(await screen.findByText('Managed core runtime')).toBeInTheDocument()
    expect(await screen.findByText('exit status 1')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('link', { name: 'Network Test' }))
    expect(await screen.findByRole('heading', { name: 'Network Test' })).toBeInTheDocument()
  })

  it('returns to login when the stored session is rejected', async () => {
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'stale-session', expiresAt: '2099-01-01T00:00:00Z' }))
    vi.stubGlobal('fetch', vi.fn(async () => Response.json({ error: { code: 'UNAUTHORIZED', message: 'a valid administrator session is required' } }, { status: 401 })))

    render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <I18nProvider><SessionProvider><App /></SessionProvider></I18nProvider>
      </QueryClientProvider>,
    )

    expect(await screen.findByLabelText('Sempre address')).toBeInTheDocument()
    expect(sessionStorage.getItem('sempre.session.v1')).toBeNull()
  })
})
