import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import type { SubscriptionCatalogResponse, SubscriptionProfile } from '../lib/types'
import { Subscriptions } from './Subscriptions'

vi.mock('../features/subscriptions/toolbox/MessageBridge', () => ({ MessageBridge: () => null }))
vi.mock('../features/subscriptions/toolbox/ProxyDebugModal', () => ({ default: () => null }))
vi.mock('../features/subscriptions/toolbox/ProxyPreviewModal', () => ({ default: () => null }))
vi.mock('../features/subscriptions/toolbox/ProxySubscribeEditor', async () => {
  const { useState } = await import('react')
  function MockSubscriptionEditor({ diagnostics }: { diagnostics: ReactNode }) {
    const [showDiagnostics, setShowDiagnostics] = useState(false)
    return <div data-testid="subscription-editor"><button type="button" onClick={() => setShowDiagnostics(true)}>Open diagnostics</button>{showDiagnostics ? diagnostics : null}</div>
  }
  return { default: MockSubscriptionEditor }
})

type RecordedRequest = { url: string; method: string; body: unknown }

let profiles: SubscriptionProfile[]
let activeProfileID: string
let requests: RecordedRequest[]
let restartResponse: Promise<Response> | undefined

function profile(id: string, name: string): SubscriptionProfile {
  return {
    id,
    name,
    log_level: 'info',
    editor: { rule_list: '{}', group: '[]', filter: '[]', custom_config: '[]', dns_config: '', private_access_config: '', servers: '[]' },
    sources: [],
    custom_node_ids: [],
    groups: [],
    rules: [],
    rule_providers: [],
    filters: [],
    use_system_groups: true,
    use_system_rules: true,
    use_system_filters: true,
    use_system_dns: true,
    use_system_custom_config: true,
    last_runtime_validated: false,
  }
}

function catalog(): SubscriptionCatalogResponse {
  return {
    profiles,
    active_profile_id: activeProfileID,
    schedule: { interval: '24h' },
    auto_restart: true,
    targets: [],
    defaults: { groups: [], rule_providers: [], filters: [], rules: [], dns: {} },
    editor_defaults: { rule_list: '{}', group: '[]', filter: '[]', custom_config: '[]', dns_config: '', private_access_config: '', servers: '[]' },
  }
}

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } })
}

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  return render(
    <QueryClientProvider client={client}>
      <I18nProvider><SessionProvider><Subscriptions /></SessionProvider></I18nProvider>
    </QueryClientProvider>,
  )
}

describe('Subscriptions subscription sets', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
    profiles = [profile('primary', 'Primary')]
    activeProfileID = 'primary'
    requests = []
    restartResponse = undefined
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init: RequestInit = {}) => {
      const url = String(input)
      const method = init.method || 'GET'
      const body = typeof init.body === 'string' ? JSON.parse(init.body) as Record<string, string> : undefined
      requests.push({ url, method, body })

      if (url.endsWith('/api/v1/custom-nodes')) return jsonResponse({ nodes: [] })
      if (url.endsWith('/api/v1/subscriptions') && method === 'GET') return jsonResponse(catalog())
      if (url.endsWith('/api/v1/runtime/restart') && method === 'POST') return restartResponse ?? jsonResponse({ action: 'restart', status: {} }, 202)
      if (url.endsWith('/api/v1/subscriptions') && method === 'POST') {
        const created = profile(`set-${profiles.length + 1}`, body?.name || '')
        profiles = [...profiles, created]
        return jsonResponse(created, 201)
      }

      const match = url.match(/\/api\/v1\/subscriptions\/([^/]+)(?:\/(activate|refresh))?$/)
      if (match) {
        const id = decodeURIComponent(match[1])
        if (method === 'PATCH') {
          const renamed = { ...profiles.find((item) => item.id === id)!, name: body?.name || '' }
          profiles = profiles.map((item) => item.id === id ? renamed : item)
          return jsonResponse(renamed)
        }
        if (method === 'POST' && match[2] === 'activate') {
          activeProfileID = id
          return jsonResponse({ change: { changed: true, message: 'Subscription set activated.' } })
        }
        if (method === 'POST' && match[2] === 'refresh') {
          return jsonResponse({ change: { changed: true, message: 'Subscription set refreshed.' } })
        }
        if (method === 'DELETE') {
          profiles = profiles.filter((item) => item.id !== id)
          return jsonResponse({ change: { changed: true, message: 'Subscription set deleted.' } })
        }
      }
      throw new Error(`Unexpected request: ${method} ${url}`)
    }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('creates, renames, activates, and deletes subscription sets through dialogs and the tab menu', async () => {
    renderPage()
    expect(await screen.findByRole('tab', { name: 'Primary' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.queryByText('New subscription set')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'New subscription set' }))
    let dialog = screen.getByRole('dialog', { name: 'New subscription set' })
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }))
    expect(dialog).toBeInTheDocument()
    await waitFor(() => expect(dialog).toHaveClass('opacity-0'))
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'New subscription set' })).not.toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: 'New subscription set' }))
    dialog = screen.getByRole('dialog', { name: 'New subscription set' })
    const createButton = within(dialog).getByRole('button', { name: 'Create' })
    expect(createButton).toBeDisabled()
    const createNameInput = within(dialog).getByLabelText('Subscription set name')
    fireEvent.change(createNameInput, { target: { value: 'Work' } })
    fireEvent.submit(createNameInput.closest('form')!)

    expect(await screen.findByRole('tab', { name: 'Work' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getAllByRole('tab')).toHaveLength(2)
    expect(activeProfileID).toBe('primary')
    expect(requests).toContainEqual(expect.objectContaining({ method: 'POST', body: { name: 'Work' } }))

    fireEvent.click(screen.getByRole('button', { name: 'Manage subscription set: Work' }))
    fireEvent.click(await screen.findByRole('button', { name: 'Rename subscription set' }))
    dialog = screen.getByRole('dialog', { name: 'Rename subscription set' })
    const nameInput = within(dialog).getByLabelText('Subscription set name')
    expect(nameInput).toHaveValue('Work')
    fireEvent.change(nameInput, { target: { value: 'Personal' } })
    fireEvent.click(within(dialog).getByRole('button', { name: 'Rename subscription set' }))

    expect(await screen.findByRole('tab', { name: 'Personal' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.queryByRole('tab', { name: 'Work' })).not.toBeInTheDocument()
    expect(requests).toContainEqual(expect.objectContaining({ method: 'PATCH', body: { name: 'Personal' } }))

    fireEvent.click(screen.getByRole('button', { name: 'Manage subscription set: Personal' }))
    fireEvent.click(await screen.findByRole('button', { name: 'Activate subscription set' }))
    await waitFor(() => expect(activeProfileID).toBe('set-2'))
    const activeTab = await screen.findByRole('tab', { name: 'Personal' })
    expect(activeTab.querySelector('[aria-hidden="true"]')).toBeInTheDocument()
    expect(screen.queryByText('Active subscription set')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('tab', { name: 'Primary' }))
    fireEvent.click(screen.getByRole('button', { name: 'Manage subscription set: Primary' }))
    fireEvent.click(await screen.findByRole('button', { name: 'Delete subscription set' }))
    dialog = screen.getByRole('dialog', { name: 'Delete subscription set' })
    expect(within(dialog).getByText(/Permanently delete subscription set: Primary/)).toBeInTheDocument()
    fireEvent.click(within(dialog).getByRole('button', { name: 'Delete subscription set' }))

    await waitFor(() => expect(screen.queryByRole('tab', { name: 'Primary' })).not.toBeInTheDocument())
    expect(screen.getAllByRole('tab')).toHaveLength(1)
    expect(screen.getByRole('tab', { name: 'Personal' })).toHaveAttribute('aria-selected', 'true')
    expect(requests).toContainEqual(expect.objectContaining({ method: 'DELETE', url: 'http://sempre.test/api/v1/subscriptions/primary' }))
  })

  it('rejects duplicate names and disables invalid actions for the active set', async () => {
    profiles = [profile('primary', 'Primary'), profile('secondary', 'Secondary')]
    renderPage()
    await screen.findByRole('tab', { name: 'Primary' })

    fireEvent.click(screen.getByRole('tab', { name: 'Secondary' }))
    fireEvent.click(screen.getByRole('button', { name: 'Manage subscription set: Secondary' }))
    fireEvent.click(await screen.findByRole('button', { name: 'Rename subscription set' }))
    const dialog = screen.getByRole('dialog', { name: 'Rename subscription set' })
    fireEvent.change(within(dialog).getByLabelText('Subscription set name'), { target: { value: ' primary ' } })
    fireEvent.click(within(dialog).getByRole('button', { name: 'Rename subscription set' }))
    expect(within(dialog).getByText('That subscription set name is already in use.')).toBeInTheDocument()
    expect(requests.filter((request) => request.method === 'PATCH')).toHaveLength(0)
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }))

    fireEvent.click(screen.getByRole('tab', { name: 'Primary' }))
    fireEvent.click(screen.getByRole('button', { name: 'Manage subscription set: Primary' }))
    expect(await screen.findByRole('button', { name: /Activate subscription set/ })).toBeDisabled()
    expect(screen.getByText('Already the active subscription set')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Delete subscription set/ })).toBeDisabled()
    expect(screen.getByText('The active subscription set cannot be deleted')).toBeInTheDocument()
  })

  it('shows the editor directly and runs update and restart from the page header', async () => {
    profiles = [
      { ...profile('primary', 'Primary'), last_compiler_target: 'sing-box-v13', last_result: 'source response contains no proxy nodes' },
      profile('secondary', 'Secondary'),
    ]
    let resolveRestart: ((response: Response) => void) | undefined
    restartResponse = new Promise<Response>((resolve) => { resolveRestart = resolve })
    renderPage()

    await screen.findByRole('tab', { name: 'Primary' })
    expect(screen.getByTestId('subscription-editor')).toBeInTheDocument()
    expect(screen.queryByText('Active subscription set')).not.toBeInTheDocument()
    expect(screen.queryByText(/sing-box-v13.*source response contains no proxy nodes/)).not.toBeInTheDocument()
    expect(screen.queryByText('source response contains no proxy nodes')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Open diagnostics' }))
    expect(screen.getByText('source response contains no proxy nodes')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('tab', { name: 'Secondary' }))
    fireEvent.click(screen.getByRole('button', { name: 'Update now' }))
    await waitFor(() => expect(requests).toContainEqual(expect.objectContaining({
      method: 'POST',
      url: 'http://sempre.test/api/v1/subscriptions/secondary/refresh',
    })))

    const restart = screen.getByRole('button', { name: 'Restart core now' })
    fireEvent.click(restart)
    await waitFor(() => expect(restart).toBeDisabled())
    await waitFor(() => expect(requests).toContainEqual(expect.objectContaining({
      method: 'POST',
      url: 'http://sempre.test/api/v1/runtime/restart',
    })))

    resolveRestart?.(jsonResponse({ action: 'restart', status: {} }, 202))
    expect(await screen.findByRole('status')).toHaveTextContent('Operation accepted')
    await waitFor(() => expect(restart).toBeEnabled())
  })

  it('shows restart failures as an error notice', async () => {
    restartResponse = Promise.resolve(jsonResponse({ error: { code: 'RUNTIME_ERROR', message: 'Managed core is unavailable' } }, 503))
    renderPage()

    fireEvent.click(await screen.findByRole('button', { name: 'Restart core now' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Managed core is unavailable')
  })
})
