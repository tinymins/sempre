import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { AcmeContentBoundary } from '../components/AcmeContentBoundary'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { CustomNodes } from './CustomNodes'

vi.mock('@uiw/react-codemirror', () => ({
  default: ({ value, onChange }: { value: string; onChange: (value: string) => void }) => (
    <textarea aria-label="Node JSON" value={value} onChange={(event) => onChange(event.target.value)} />
  ),
}))

describe('CustomNodes', () => {
  let requests: Array<{ method: string; body?: unknown }>

  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
    requests = []
    vi.stubGlobal('fetch', vi.fn(async (_input: RequestInfo | URL, init: RequestInit = {}) => {
      const method = init.method ?? 'GET'
      requests.push({ method, body: typeof init.body === 'string' ? JSON.parse(init.body) : undefined })
      return new Response(JSON.stringify(method === 'GET' ? { nodes: [] } : {}), { status: method === 'GET' ? 200 : 201, headers: { 'Content-Type': 'application/json' } })
    }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('uses the shared Modal and validates JSON before saving', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
    render(
      <QueryClientProvider client={client}>
        <I18nProvider><SessionProvider><AcmeContentBoundary><CustomNodes /></AcmeContentBoundary></SessionProvider></I18nProvider>
      </QueryClientProvider>,
    )

    const addButtons = await screen.findAllByRole('button', { name: 'Add node' })
    fireEvent.click(addButtons[0])
    const dialog = await screen.findByRole('dialog', { name: 'Add node' })
    expect(dialog).toHaveStyle({ width: '900px' })

    const editor = within(dialog).getByRole('textbox', { name: 'Node JSON' })
    fireEvent.change(editor, { target: { value: '{' } })
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save' }))
    expect(requests.filter((request) => request.method === 'POST')).toHaveLength(0)
    expect(within(dialog).getByText(/JSON at position/)).toBeInTheDocument()

    fireEvent.change(editor, { target: { value: '{ "name": "edge", "type": "socks5", "server": "127.0.0.1", "port": 1080 }' } })
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(requests.filter((request) => request.method === 'POST')).toHaveLength(1))
  })
})
