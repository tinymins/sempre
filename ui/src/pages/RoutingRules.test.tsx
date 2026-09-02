import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { RoutingRules } from './RoutingRules'

const settings = {
  schema: 3,
  revision: 1,
  enabled: true,
  direct_upstreams: [],
  rule_sets: [{
    id: 'direct-sites',
    name: 'Direct sites',
    mode: 'direct',
    domains: [{ id: 'domain-1', domain: 'old.example', include_subdomains: false }],
  }],
  reject_https: true,
  rewrites: [],
  query_log_enabled: true,
  query_log_max_entries: 2000,
}

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><RoutingRules /></SessionProvider></I18nProvider></QueryClientProvider>)
}

function response(body: unknown) {
  return new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } })
}

describe('RoutingRules', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url.endsWith('/api/v1/dns/settings') && init?.method === 'PUT') return response({})
      if (url.endsWith('/api/v1/dns/settings')) return response({ settings, status: { domestic_domain_count: 77072 } })
      if (url.endsWith('/api/v1/runtime/proxies')) return response([])
      if (url.endsWith('/api/v1/runtime/status')) return response({ pending: true, pending_changes: [], runtime_state: 'running' })
      return response({})
    }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('edits rule-set metadata and domain rows through dialogs before saving', async () => {
    renderPage()

    expect(await screen.findAllByText('Mainland China domains')).toHaveLength(2)
    fireEvent.click(screen.getByRole('button', { name: /Direct sites/ }))
    expect(screen.getByText('old.example')).toBeInTheDocument()
    expect(screen.queryByDisplayValue('old.example')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Rule set settings' }))
    const settingsDialog = screen.getByRole('dialog', { name: 'Rule set settings' })
    fireEvent.change(within(settingsDialog).getByRole('textbox', { name: 'Name' }), { target: { value: 'Private sites' } })
    fireEvent.click(within(settingsDialog).getByRole('button', { name: 'Apply' }))

    fireEvent.click(screen.getByRole('button', { name: 'Add rule' }))
    const addDialog = screen.getByRole('dialog', { name: 'Add rule' })
    fireEvent.change(within(addDialog).getByRole('textbox', { name: 'Domain' }), { target: { value: '*.Example.COM.' } })
    fireEvent.click(within(addDialog).getByRole('button', { name: 'Add' }))
    expect(screen.getByText('example.com')).toBeInTheDocument()

    fireEvent.click(screen.getAllByTitle('Edit')[0])
    const editDialog = screen.getByRole('dialog', { name: 'Edit rule' })
    fireEvent.change(within(editDialog).getByRole('textbox', { name: 'Domain' }), { target: { value: 'edited.example' } })
    fireEvent.click(within(editDialog).getByRole('button', { name: 'Save' }))
    expect(screen.getByText('edited.example')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    await vi.waitFor(() => {
      const request = vi.mocked(fetch).mock.calls.find(([input, init]) => String(input).endsWith('/api/v1/dns/settings') && init?.method === 'PUT')
      const body = JSON.parse(String(request?.[1]?.body))
      expect(body.rule_sets).toEqual([expect.objectContaining({
        name: 'Private sites',
        domains: [
          expect.objectContaining({ domain: 'edited.example', include_subdomains: false }),
          expect.objectContaining({ domain: 'example.com', include_subdomains: true }),
        ],
      })])
    })
  })

  it('switches a recognized proxy rule-set group from the autocomplete', async () => {
    const proxySettings = { ...settings, rule_sets: [{ ...settings.rule_sets[0], name: 'Streaming', mode: 'proxy' }] }
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.endsWith('/api/v1/dns/settings')) return response({ settings: proxySettings, status: { domestic_domain_count: 77072 } })
      if (url.endsWith('/api/v1/runtime/proxies/select')) return response({})
      if (url.endsWith('/api/v1/runtime/proxies')) return response([{ name: 'DNS · Streaming', type: 'Selector', all: ['HK-01', 'JP-01'], now: 'HK-01' }])
      if (url.endsWith('/api/v1/runtime/status')) return response({ pending: false, pending_changes: [], runtime_state: 'running' })
      return response({})
    }))
    renderPage()

    fireEvent.click(await screen.findByRole('button', { name: /Streaming/ }))
    expect(screen.getByText('Quick proxy selection')).toBeInTheDocument()
    const selector = screen.getByRole('textbox', { name: 'Proxy node' })
    fireEvent.mouseDown(selector)
    fireEvent.focus(selector)
    fireEvent.click(await screen.findByRole('option', { name: 'JP-01' }))
    await vi.waitFor(() => expect(fetch).toHaveBeenCalledWith('http://sempre.test/api/v1/runtime/proxies/select', expect.objectContaining({
      method: 'POST',
      body: JSON.stringify({ group: 'DNS · Streaming', proxy: 'JP-01' }),
    })))
  })

  it('offers the shared restart action when the running core lacks the proxy group', async () => {
    const proxySettings = { ...settings, rule_sets: [{ ...settings.rule_sets[0], name: 'Changed name', mode: 'proxy' }] }
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.endsWith('/api/v1/dns/settings')) return response({ settings: proxySettings, status: { domestic_domain_count: 77072 } })
      if (url.endsWith('/api/v1/runtime/proxies')) return response([])
      if (url.endsWith('/api/v1/runtime/status')) return response({ pending: true, pending_changes: [], runtime_state: 'running' })
      return response({})
    }))
    renderPage()

    fireEvent.click(await screen.findByRole('button', { name: /Changed name/ }))
    expect(screen.getByText(/has not recognized this proxy group/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Restart core' })).toBeInTheDocument()
  })
})
