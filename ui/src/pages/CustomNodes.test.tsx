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
  const select = async (dialog: HTMLElement, ...names: string[]) => {
    fireEvent.click(within(dialog).getByRole('combobox'))
    const list = await screen.findByRole('listbox')
    for (const name of names) fireEvent.click(within(list).getByText(name, { exact: true }))
    fireEvent.keyDown(within(dialog).getByRole('combobox'), { key: 'Escape' })
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

  it('uses the shared Modal and validates JSON before saving', async () => {
    renderPage()
    const dialog = await openNew()
    expect(dialog).toHaveStyle({ width: '900px' })

    const editor = within(dialog).getByRole('textbox', { name: 'Node JSON' })
    fireEvent.change(editor, { target: { value: '{' } })
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save' }))
    expect(requests.filter((request) => request.method === 'POST')).toHaveLength(0)
    expect(within(dialog).getByText(/JSON at position/)).toBeInTheDocument()

    fireEvent.change(editor, { target: { value: '{ "name": "edge", "type": "socks5", "server": "127.0.0.1", "port": 1080 }' } })
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(requests.filter((request) => request.method === 'POST')).toHaveLength(1))
    await waitFor(() => expect(dialog).toHaveClass('opacity-0'))
    expect(editor).toHaveValue('{ "name": "edge", "type": "socks5", "server": "127.0.0.1", "port": 1080 }')
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Add node' })).not.toBeInTheDocument())

    fireEvent.click((await screen.findAllByRole('button', { name: 'Add node' }))[0])
    const reopened = await screen.findByRole('dialog', { name: 'Add node' })
    expect((within(reopened).getByRole('textbox', { name: 'Node JSON' }) as HTMLTextAreaElement).value).toContain('"type": "vless"')
  })

  it('defaults new nodes to all editable profiles with one write request', async () => {
    renderPage()
    const dialog = await openNew()
    expect(within(dialog).getByRole('combobox')).toHaveTextContent('Primary')
    expect(within(dialog).getByRole('combobox')).toHaveTextContent('Work')
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(profiles[0].custom_node_ids).toEqual(['other-node', 'node-1'])
    expect(profiles[1].custom_node_ids).toEqual(['node-1'])
    expect(profiles[2].custom_node_ids).toEqual([])
    const writes = requests.filter((request) => request.method !== 'GET')
    expect(writes).toHaveLength(1)
    expect(writes[0]).toMatchObject({ method: 'POST', path: '/custom-nodes', body: { subscription_ids: ['Primary', 'Work'] } })
    expect(Object.keys(nodes[0]).sort()).toEqual(['id', 'name', 'proxy'])
    expect(screen.getByText('Primary, Work')).toBeInTheDocument()
  })

  it('reads profile-side assignments and can remove only this node from a profile', async () => {
    nodes = [{ id: 'edge', name: 'Edge', proxy: exampleProxy }]
    profiles[0].custom_node_ids.push('edge')
    const client = renderPage()
    await screen.findByText('Edge')
    // A save from the subscription page updates the same cached catalog.
    profiles[1].custom_node_ids = ['edge']
    await client.invalidateQueries({ queryKey: ['subscriptions'] })
    fireEvent.click(screen.getByRole('button', { name: 'Edit node' }))
    const dialog = await screen.findByRole('dialog', { name: 'Edit node' })
    expect(within(dialog).getByRole('combobox')).toHaveTextContent('Primary')
    expect(within(dialog).getByRole('combobox')).toHaveTextContent('Work')
    await select(dialog, 'Primary')
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(profiles[0].custom_node_ids).toEqual(['other-node'])
    expect(profiles[1].custom_node_ids).toEqual(['edge'])
    const writes = requests.filter((request) => request.method === 'PUT')
    expect(writes).toHaveLength(1)
    expect(writes[0]).toMatchObject({ path: '/custom-nodes/edge', body: { subscription_ids: ['Work'] } })
  })

  it('keeps remote profiles read-only and discards canceled selections', async () => {
    renderPage()
    const dialog = await openNew()
    fireEvent.click(within(dialog).getByRole('combobox'))
    const remote = within(await screen.findByRole('listbox')).getByText('Remote (Read-only)')
    fireEvent.click(remote)
    expect(within(dialog).getByRole('combobox')).not.toHaveTextContent('Remote')
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(requests.every((request) => request.method === 'GET')).toBe(true)
    const reopened = await openNew()
    expect(within(reopened).getByRole('combobox')).toHaveTextContent('Primary')
    expect(within(reopened).getByRole('combobox')).toHaveTextContent('Work')
    await select(reopened, 'Primary', 'Work')
    fireEvent.click(within(reopened).getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(requests.find((request) => request.method === 'POST')).toMatchObject({ body: { subscription_ids: [] } })
  })

  it('retains selections after a failed batch and retries the single request', async () => {
    renderPage()
    const dialog = await openNew()
    failProfile = 'Work'
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save' }))
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('Profile save failed')
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

const exampleProxy = { name: 'edge', type: 'socks5', server: '127.0.0.1', port: 1080 }
