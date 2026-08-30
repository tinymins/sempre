import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { AcmeContentBoundary } from '../../components/AcmeContentBoundary'
import { I18nProvider } from '../../lib/i18n'
import { SessionProvider } from '../../lib/session'
import { ThemeProvider } from '../../lib/theme'
import { ServerApp } from './ServerApp'
import { newServerProfile } from './server-api'

vi.mock('@monaco-editor/react', () => ({
  default: ({ value = '', onChange, options }: { value?: string; onChange?: (value: string) => void; options?: { readOnly?: boolean } }) => (
    <textarea aria-label="JSONC editor" value={value} readOnly={options?.readOnly} onChange={(event) => onChange?.(event.target.value)} />
  ),
  loader: { config: vi.fn() },
}))
vi.mock('monaco-editor', () => ({}))

function response(body: unknown) {
  return new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } })
}

function renderServerApp() {
  return render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><I18nProvider><SessionProvider><ThemeProvider><AcmeContentBoundary><ServerApp /></AcmeContentBoundary></ThemeProvider></SessionProvider></I18nProvider></QueryClientProvider>)
}

describe('ServerApp', () => {
  beforeEach(() => {
    window.location.hash = '#/subscriptions/profile-1'
    localStorage.setItem('sempre.server.session.v1', JSON.stringify({
      token: 'server-token', expiresAt: '2099-01-01T00:00:00Z', user: { id: 'user-1', email: 'viewer@example.com' },
    }))
    localStorage.setItem('sempre.locale', 'en')
    const document = newServerProfile('Team profile')
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url === '/api/v1/auth/me') return response({ id: 'user-1', email: 'viewer@example.com' })
      if (url === '/api/v1/profiles') return response([{
        id: 'profile-1', owner_id: 'owner-1', revision: 5, name: 'Team profile', document, role: 'viewer', updated_at: '2026-08-24T00:00:00Z',
      }])
      if (url === '/api/v1/profiles/profile-1') return response({
        id: 'profile-1', owner_id: 'owner-1', revision: 5, name: 'Team profile', document, role: 'viewer', updated_at: '2026-08-24T00:00:00Z',
      })
      if (url === '/api/v1/targets') return response([{ format: 'sing-box-v13', version: '13', platform: 'default' }])
      if (url === '/api/v1/custom-nodes') return response([])
      if (url === '/api/v1/profiles/profile-1/stats') return response({ total_accesses: 0, today_accesses: 0, by_target: [], recent_accesses: [] })
      if (url === '/api/v1/profiles/profile-1/refresh') return response({ enabled: false, interval_minutes: 1440, targets: ['sing-box-v13'], last_refresh_status: 'never' })
      throw new Error(`Unexpected request: ${url}`)
    }))
  })

  afterEach(() => {
    cleanup()
    localStorage.removeItem('sempre.server.session.v1')
    localStorage.removeItem('sempre.locale')
    window.location.hash = ''
    vi.unstubAllGlobals()
  })

  it('opens the manifest edit route as a read-only page for viewers', async () => {
    renderServerApp()
    expect(await screen.findByRole('link', { name: 'Overview' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Subscriptions' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Custom Nodes' })).toBeInTheDocument()
    expect(await screen.findByRole('heading', { name: 'Team profile' })).toBeInTheDocument()
    expect(screen.getAllByText('viewer')).not.toHaveLength(0)
    expect(screen.getByText('This shared profile is read-only.')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument()
    expect(screen.queryByText('Custom node library')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Basic' })).toBeInTheDocument()
  })

  it('persists owner refresh settings and publishes immediately', async () => {
    const document = newServerProfile('Owned profile')
    const requests: { url: string; init?: RequestInit }[] = []
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      requests.push({ url, init })
      if (url === '/api/v1/auth/me') return response({ id: 'user-1', email: 'viewer@example.com' })
      if (url === '/api/v1/profiles') return response([{
        id: 'profile-1', owner_id: 'user-1', revision: 2, name: 'Owned profile', document, role: 'owner', updated_at: '2026-08-24T00:00:00Z',
      }])
      if (url === '/api/v1/profiles/profile-1') return response({
        id: 'profile-1', owner_id: 'user-1', revision: 2, name: 'Owned profile', document, role: 'owner', updated_at: '2026-08-24T00:00:00Z',
      })
      if (url === '/api/v1/targets') return response([{ format: 'sing-box-v13', version: '13', platform: 'default' }])
      if (url === '/api/v1/custom-nodes' || url.endsWith('/shares') || url.endsWith('/members')) return response([])
      if (url.endsWith('/stats')) return response({ total_accesses: 0, today_accesses: 0, by_target: [], recent_accesses: [] })
      if (url.endsWith('/refresh') && init?.method === 'POST') return response([{ content: '{}', node_count: 1, artifact_hash: 'published-hash', field_diffs: [], diagnostics: [] }])
      if (url.endsWith('/refresh')) {
        const body = init?.body ? JSON.parse(String(init.body)) as { enabled?: boolean; interval_minutes?: number } : {}
        return response({ enabled: body.enabled ?? false, interval_minutes: body.interval_minutes ?? 1440, targets: ['sing-box-v13'], last_refresh_status: init?.method === 'PUT' ? 'success' : 'never' })
      }
      throw new Error(`Unexpected request: ${url}`)
    }))

    renderServerApp()
    expect(screen.queryByRole('checkbox', { name: 'Restart after scheduled updates' })).not.toBeInTheDocument()
    fireEvent.click(await screen.findByRole('button', { name: 'Diagnostics' }))
    const automatic = await screen.findByRole('checkbox', { name: 'Automatically refresh and publish this target' })
    fireEvent.click(automatic)
    await waitFor(() => expect(requests.some(({ url, init }) => url.endsWith('/refresh') && init?.method === 'PUT' && String(init.body).includes('"enabled":true'))).toBe(true))

    fireEvent.click(screen.getByRole('button', { name: 'Refresh and publish now' }))
    expect(await screen.findByText(/Published 1 nodes/)).toBeInTheDocument()
    expect(requests.some(({ url, init }) => url.endsWith('/refresh') && init?.method === 'POST')).toBe(true)
  })
})
