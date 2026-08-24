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
  const { forwardRef, useImperativeHandle, useState } = await import('react')
  const MockSubscriptionEditor = forwardRef(function MockSubscriptionEditor({ diagnostics, profile, onSave, onSaveStateChange }: { diagnostics: ReactNode; profile: SubscriptionProfile; onSave: (candidate: SubscriptionProfile) => Promise<void>; onSaveStateChange?: (state: { profileID: string; dirty: boolean; saving: boolean }) => void }, ref) {
    const [showDiagnostics, setShowDiagnostics] = useState(false)
    const [remark, setRemark] = useState(profile.remark ?? '')
    useImperativeHandle(ref, () => ({
      saveNow: () => {
        onSaveStateChange?.({ profileID: profile.id, dirty: true, saving: true })
        void onSave({ ...profile, remark }).then(() => onSaveStateChange?.({ profileID: profile.id, dirty: false, saving: false }))
      },
    }))
    return <div data-testid="subscription-editor"><button type="button" onClick={() => setShowDiagnostics(true)}>Open diagnostics</button><button type="button" onClick={() => { setRemark('Edited profile'); onSaveStateChange?.({ profileID: profile.id, dirty: true, saving: false }) }}>Edit profile</button>{showDiagnostics ? diagnostics : null}</div>
  })
  return { default: MockSubscriptionEditor }
})

type RecordedRequest = { url: string; method: string; body: unknown }

let profiles: SubscriptionProfile[]
let activeProfileID: string
let requests: RecordedRequest[]
let restartResponse: Promise<Response> | undefined
let catalogRefreshResponse: Promise<Response> | undefined
let catalogReads: number
let runtimePending: boolean
let runtimeStatusResponse: Record<string, unknown>
let restartFinalStatus: Record<string, unknown> | undefined

function profile(id: string, name: string): SubscriptionProfile {
  return {
    id,
	revision: 1,
    name,
    mode: 'local',
    log_level: 'info',
    editor: { rule_list: '{}', group: '[]', filter: '[]', custom_config: '[]', dns_config: '', private_access_config: '', servers: '[]' },
    sources: [],
    custom_node_ids: [],
    groups: [],
    rules: [],
    rule_providers: [],
    filters: [],
		core_overrides: {},
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
		configuration_context: { key: 'common', platform: 'linux', capabilities: { features: [], enum_values: {}, protocols: [] } },
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
	catalogRefreshResponse = undefined
	catalogReads = 0
    runtimePending = false
    runtimeStatusResponse = {}
    restartFinalStatus = undefined
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init: RequestInit = {}) => {
      const url = String(input)
      const method = init.method || 'GET'
      const body = typeof init.body === 'string' ? JSON.parse(init.body) as Record<string, string> : undefined
      requests.push({ url, method, body })

      if (url.endsWith('/api/v1/custom-nodes')) return jsonResponse({ nodes: [] })
      if (url.endsWith('/api/v1/subscriptions') && method === 'GET') {
		catalogReads += 1
		if (catalogReads > 1 && catalogRefreshResponse) return await catalogRefreshResponse
		return jsonResponse(catalog())
	  }
      if (url.endsWith('/api/v1/runtime/status') && method === 'GET') return jsonResponse({ ...runtimeStatusResponse, pending: runtimePending })
      if (url.endsWith('/api/v1/runtime/restart') && method === 'POST') {
        const response = await (restartResponse ?? Promise.resolve(jsonResponse({ action: 'restart', status: {} }, 202)))
        if (response.ok) {
          runtimePending = false
          if (restartFinalStatus) runtimeStatusResponse = restartFinalStatus
        }
        return response
      }
      if (url.endsWith('/api/v1/subscriptions') && method === 'POST') {
        const created = {
          ...profile(`set-${profiles.length + 1}`, body?.name || ''),
          mode: body?.mode === 'remote' ? 'remote' as const : 'local' as const,
          remote: body?.mode === 'remote' ? {
            manifest_url: body.manifest_url,
            edit_url: 'https://server.example/subscriptions/team',
            server_profile: 'Team profile',
            server_revision: 4,
            artifact_sha256: 'a'.repeat(64),
            target: 'sing-box-v13',
            node_count: 12,
            last_synced_at: '2026-08-24T00:00:00Z',
          } : undefined,
        }
        profiles = [...profiles, created]
        return jsonResponse(created, 201)
      }

      const match = url.match(/\/api\/v1\/subscriptions\/([^/]+)(?:\/(activate|refresh))?$/)
      if (match) {
        const id = decodeURIComponent(match[1])
        if (method === 'PUT') {
          profiles = profiles.map((item) => item.id === id ? { ...item, ...body } : item)
          runtimePending = true
          return jsonResponse({ change: { Changed: true, NeedsRestart: true, Message: 'Subscription set saved.' } })
        }
        if (method === 'PATCH') {
          const renamed = { ...profiles.find((item) => item.id === id)!, name: body?.name || '' }
          profiles = profiles.map((item) => item.id === id ? renamed : item)
          return jsonResponse(renamed)
        }
        if (method === 'POST' && match[2] === 'activate') {
          activeProfileID = id
          return jsonResponse({ change: { Changed: true, NeedsRestart: false, Message: 'Subscription set activated.' } })
        }
        if (method === 'POST' && match[2] === 'refresh') {
          return jsonResponse({ change: { Changed: true, NeedsRestart: false, Message: 'Subscription set refreshed.' } })
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
    let dialog = await screen.findByRole('dialog', { name: 'New subscription set' }, { timeout: 3000 })
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }))
    expect(dialog).toBeInTheDocument()
    await waitFor(() => expect(dialog).toHaveClass('opacity-0'))
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'New subscription set' })).not.toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: 'New subscription set' }))
    dialog = await screen.findByRole('dialog', { name: 'New subscription set' }, { timeout: 3000 })
    const createButton = within(dialog).getByRole('button', { name: 'Create' })
    expect(createButton).toBeDisabled()
    const createNameInput = within(dialog).getByLabelText('Subscription set name')
    fireEvent.change(createNameInput, { target: { value: 'Work' } })
    fireEvent.submit(createNameInput.closest('form')!)

    expect(await screen.findByRole('tab', { name: 'Work' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getAllByRole('tab')).toHaveLength(2)
    expect(activeProfileID).toBe('primary')
    expect(requests).toContainEqual(expect.objectContaining({ method: 'POST', body: { name: 'Work' } }))
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'New subscription set' })).not.toBeInTheDocument(), { timeout: 3000 })

    fireEvent.click(screen.getByRole('button', { name: 'Manage subscription set: Work' }))
    fireEvent.click(await screen.findByRole('button', { name: 'Rename subscription set' }))
    dialog = await screen.findByRole('dialog', { name: 'Rename subscription set' }, { timeout: 3000 })
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

  it('creates a remote read-only subscription and links back to its server editor', async () => {
    renderPage()
    await screen.findByRole('tab', { name: 'Primary' })
    fireEvent.click(screen.getByRole('button', { name: 'New subscription set' }))
    const dialog = await screen.findByRole('dialog', { name: 'New subscription set' })
    fireEvent.change(within(dialog).getByLabelText('Subscription set name'), { target: { value: 'Team' } })
    fireEvent.click(within(dialog).getByRole('combobox'))
    fireEvent.click(await screen.findByText('Remote read-only'))
    fireEvent.change(within(dialog).getByLabelText('Remote manifest URL'), { target: { value: 'https://server.example/api/v1/public/subscriptions/token' } })
    fireEvent.click(within(dialog).getByRole('button', { name: 'Create' }))

    expect(await screen.findByRole('tab', { name: 'Team' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByText('Remote subscription configuration')).toBeInTheDocument()
    expect(screen.getByText('Read-only')).toBeInTheDocument()
    expect(screen.queryByTestId('subscription-editor')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Edit on server' })).toBeEnabled()
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'New subscription set' })).not.toBeInTheDocument())
    expect(screen.getByDisplayValue('https://server.example/api/v1/public/subscriptions/token')).toHaveAttribute('readonly')
    expect(requests).toContainEqual(expect.objectContaining({
      method: 'POST',
      body: { name: 'Team', mode: 'remote', manifest_url: 'https://server.example/api/v1/public/subscriptions/token' },
    }))
  })

  it('shows the editor directly and confirms update and restart from the page header', async () => {
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

    const headerActions = screen.getByRole('heading', { name: 'Subscriptions' }).parentElement?.nextElementSibling
    expect(within(headerActions as HTMLElement).getAllByRole('button').map((button) => button.textContent)).toEqual([
      'Save',
      'Update subscription',
      'Restart core',
    ])
    const saveButton = screen.getByRole('button', { name: 'Save' })
    expect(saveButton).toBeDisabled()
    fireEvent.click(screen.getByRole('button', { name: 'Edit profile' }))
    expect(saveButton).toBeEnabled()
	catalogRefreshResponse = new Promise<Response>(() => undefined)
    fireEvent.click(saveButton)
    await waitFor(() => expect(requests).toContainEqual(expect.objectContaining({
      method: 'PUT',
      url: 'http://sempre.test/api/v1/subscriptions/primary',
      body: expect.objectContaining({ remark: 'Edited profile' }),
    })))
	await waitFor(() => expect(saveButton).toBeDisabled())
    const restart = screen.getByRole('button', { name: 'Restart core' })
    await waitFor(() => expect(restart.querySelector('svg.lucide-circle-alert')).toBeInTheDocument())
    fireEvent.mouseEnter(restart)
    expect(await screen.findByRole('tooltip')).toHaveTextContent('Configuration changed. Restart the core to apply it.')

    fireEvent.click(screen.getByRole('button', { name: 'Open diagnostics' }))
    expect(screen.getByText('source response contains no proxy nodes')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('tab', { name: 'Secondary' }))
    fireEvent.click(screen.getByRole('button', { name: 'Update subscription' }))
    let actionDialog = screen.getByRole('dialog', { name: 'Update this subscription?' })
    expect(within(actionDialog).getByText('Fetch the enabled sources in Secondary, then regenerate and validate its configuration. If this is the active subscription, changes are staged until the core next restarts. This update does not restart the core or interrupt current proxy traffic.')).toBeInTheDocument()
    expect(requests).not.toContainEqual(expect.objectContaining({
      method: 'POST',
      url: 'http://sempre.test/api/v1/subscriptions/secondary/refresh',
    }))
    fireEvent.click(within(actionDialog).getByRole('button', { name: 'Cancel' }))
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Update this subscription?' })).not.toBeInTheDocument())
    expect(requests).not.toContainEqual(expect.objectContaining({
      method: 'POST',
      url: 'http://sempre.test/api/v1/subscriptions/secondary/refresh',
    }))

    fireEvent.click(screen.getByRole('button', { name: 'Update subscription' }))
    actionDialog = screen.getByRole('dialog', { name: 'Update this subscription?' })
    fireEvent.click(within(actionDialog).getByRole('button', { name: 'Update subscription' }))
    await waitFor(() => expect(requests).toContainEqual(expect.objectContaining({
      method: 'POST',
      url: 'http://sempre.test/api/v1/subscriptions/secondary/refresh',
    })))

    fireEvent.click(restart)
    actionDialog = screen.getByRole('dialog', { name: 'Restart the core?' })
    expect(within(actionDialog).getByText('Stop and start the managed core, applying any staged configuration. Existing proxy connections and traffic will be interrupted briefly; Sempre Service, the Web console, and the API will remain online.')).toBeInTheDocument()
    expect(requests).not.toContainEqual(expect.objectContaining({
      method: 'POST',
      url: 'http://sempre.test/api/v1/runtime/restart',
    }))
    fireEvent.click(within(actionDialog).getByRole('button', { name: 'Restart core' }))
    await waitFor(() => expect(restart).toBeDisabled())
    await waitFor(() => expect(requests).toContainEqual(expect.objectContaining({
      method: 'POST',
      url: 'http://sempre.test/api/v1/runtime/restart',
    })))

    resolveRestart?.(jsonResponse({ action: 'restart', status: {} }, 202))
    expect(await screen.findByRole('status')).toHaveTextContent('Operation accepted')
    await waitFor(() => expect(restart).toBeEnabled())
    await waitFor(() => expect(restart.querySelector('svg.lucide-rotate-cw')).toBeInTheDocument())
  })

  it('shows restart failures as an error notice', async () => {
    restartResponse = Promise.resolve(jsonResponse({ error: { code: 'RUNTIME_ERROR', message: 'Managed core is unavailable' } }, 503))
    renderPage()

    fireEvent.click(await screen.findByRole('button', { name: 'Restart core' }))
    const dialog = screen.getByRole('dialog', { name: 'Restart the core?' })
    fireEvent.click(within(dialog).getByRole('button', { name: 'Restart core' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Managed core is unavailable')
  })

  it('replaces the accepted restart notice with the asynchronous rollback result', async () => {
    const restored = { core: 'sing-box', ref: 'stable', version: '1.13.18', exact_reference: 'sing-box@1.13.18', config_hash: 'a'.repeat(64) }
    const failed = { ...restored, config_hash: 'b'.repeat(64) }
    runtimeStatusResponse = { runtime_state: 'running', active: restored }
    restartResponse = Promise.resolve(jsonResponse({ action: 'restart', status: { runtime_state: 'stopping', active: failed, pending: true } }, 202))
    restartFinalStatus = {
      runtime_state: 'running', active: restored, last_exit: 'exit status 1',
      last_failure: { stage: 'startup failed for sing-box@1.13.18', error: 'exit status 1', occurred_at: '2026-08-24T03:10:56Z', failed, rolled_back_to: restored },
    }
    renderPage()

    fireEvent.click(await screen.findByRole('button', { name: 'Restart core' }))
    fireEvent.click(within(screen.getByRole('dialog', { name: 'Restart the core?' })).getByRole('button', { name: 'Restart core' }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Managed core startup failed. The last working configuration was restored.')
    expect(alert).toHaveTextContent('Error: exit status 1')
    expect(alert).toHaveTextContent('Rollback: sing-box@1.13.18 · bbbbbbbb...bbbbbb → sing-box@1.13.18 · aaaaaaaa...aaaaaa')
  })
})
