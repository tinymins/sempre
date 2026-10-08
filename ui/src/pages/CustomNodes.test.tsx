import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { AcmeContentBoundary } from '../components/AcmeContentBoundary'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import type { CustomNode, SubscriptionProfile } from '../lib/types'
import { CustomNodes } from './CustomNodes'

vi.mock('@uiw/react-codemirror', () => ({
  default: ({ value, onChange }: { value: string; onChange: (value: string) => void }) => (
    <textarea aria-label="Node JSON" value={value} onChange={(event) => onChange(event.target.value)} />
  ),
}))

describe('CustomNodes', () => {
  let requests: Array<{ path: string; method: string; body?: unknown }>
  let nodes: CustomNode[]
  let profiles: SubscriptionProfile[]
  let failProfile: string

  const profile = (id: string, custom_node_ids: string[] = [], mode = 'local') => ({
    id, name: id, revision: 1, mode, custom_node_ids, sources: [], rules: ['keep this rule'],
  }) as unknown as SubscriptionProfile
  const response = (body: unknown, status = 200) => new Response(JSON.stringify(body), {
    status, headers: { 'Content-Type': 'application/json' },
  })
  const renderPage = () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><AcmeContentBoundary><CustomNodes /></AcmeContentBoundary></SessionProvider></I18nProvider></QueryClientProvider>)
    return client
  }
  const openNew = async () => {
    const buttons = await screen.findAllByRole('button', { name: 'Add node' })
    await waitFor(() => expect(buttons[0]).toBeEnabled())
    fireEvent.click(buttons[0])
    return screen.findByRole('dialog', { name: 'Add node' })
  }

  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
    requests = []
    nodes = []
    profiles = [profile('Primary', ['other-node']), profile('Work'), profile('Remote', [], 'remote')]
    failProfile = ''
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init: RequestInit = {}) => {
      const method = init.method ?? 'GET'
      const path = new URL(String(input)).pathname.replace('/api/v1', '')
      const body = typeof init.body === 'string' ? JSON.parse(init.body) : undefined
      requests.push({ path, method, body })
      if (method === 'GET') return response(path === '/custom-nodes' ? { nodes } : { profiles, configuration_context: { key: 'common' } })
      if (path === '/custom-nodes/order') {
        nodes = body.node_ids.map((id: string) => nodes.find((node) => node.id === id))
        return response({ nodes })
      }
      if (path.startsWith('/custom-nodes')) {
        if (failProfile) return response({ error: { code: 'SAVE_FAILED', message: 'Profile save failed' } }, 500)
        const { subscription_ids: selectedIDs, ...fields } = body
        const node = { ...fields, id: method === 'POST' ? `node-${nodes.length + 1}` : path.split('/').pop() }
        nodes = [...nodes.filter((item) => item.id !== node.id), node]
        profiles.forEach((item) => {
          if (item.mode === 'remote') return
          item.custom_node_ids = item.custom_node_ids.filter((id) => id !== node.id)
          if (selectedIDs.includes(item.id)) item.custom_node_ids.push(node.id)
        })
        return response(node, method === 'POST' ? 201 : 200)
      }
      throw new Error(`Unexpected request: ${method} ${path}`)
    }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('retains selections after a failed batch and retries the single request', async () => {
    renderPage()
    const dialog = await openNew()
    failProfile = 'Work'
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save' }))
    expect(await screen.findByText('Profile save failed')).toBeInTheDocument()
    expect(within(dialog).queryByRole('alert')).not.toBeInTheDocument()
    await waitFor(() => expect(within(dialog).getByRole('button', { name: 'Save' })).toBeEnabled())
    expect(nodes).toHaveLength(0)
    expect(profiles[0].custom_node_ids).toEqual(['other-node'])
    failProfile = ''
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(nodes).toHaveLength(1)
    expect(profiles[1].custom_node_ids).toEqual(['node-1'])
    expect(requests.filter((request) => request.method === 'POST')).toHaveLength(2)
    expect(requests.filter((request) => request.method === 'PUT')).toHaveLength(0)
  })
})
