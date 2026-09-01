import { cleanup, fireEvent, render, screen } from '@testing-library/react'
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
  rule_sets: [],
  reject_https: true,
  rewrites: [],
  query_log_enabled: true,
  query_log_max_entries: 2000,
}

describe('RoutingRules', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
    vi.stubGlobal('fetch', vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      const body = init?.method === 'PUT' && init.body ? JSON.parse(String(init.body)) : settings
      return new Response(JSON.stringify({
        settings: body,
        status: { domestic_domain_count: 77072 },
      }), { status: 200, headers: { 'Content-Type': 'application/json' } })
    }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('keeps domains-min built in and saves editable domain rule sets', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><RoutingRules /></SessionProvider></I18nProvider></QueryClientProvider>)

    expect(await screen.findAllByText('Mainland China domains')).toHaveLength(2)
    expect(screen.getByText(/never fetched from a runtime URL/)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Add rule set' }))
    fireEvent.change(screen.getByRole('textbox', { name: 'Name' }), { target: { value: 'Direct sites' } })
    fireEvent.change(screen.getByPlaceholderText('example.com'), { target: { value: '*.Example.COM.' } })
    fireEvent.click(screen.getByRole('button', { name: 'Add' }))
    expect(screen.getByDisplayValue('example.com')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    await vi.waitFor(() => {
      const request = vi.mocked(fetch).mock.calls.find(([, init]) => init?.method === 'PUT')
      expect(request).toBeDefined()
      const body = JSON.parse(String(request?.[1]?.body))
      expect(body.rule_sets).toEqual([
        expect.objectContaining({
          name: 'Direct sites',
          mode: 'direct',
          domains: [expect.objectContaining({ domain: 'example.com', include_subdomains: true })],
        }),
      ])
    })
  })
})
