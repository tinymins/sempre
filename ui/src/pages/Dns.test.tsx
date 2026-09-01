import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { Dns } from './Dns'

const settings = {
  settings: {
    schema: 1,
    revision: 3,
    use_system_dns: false,
    config: '{"shared":{"remoteDns":"8.8.8.8"}}',
    dns: {},
    rewrites: [],
    query_log_enabled: true,
    query_log_max_entries: 2000,
  },
  status: {
    enabled: true,
    running: true,
    core_dns_healthy: true,
    mode: 'fake-ip',
    core_upstream: '127.0.0.1:1053',
    original_upstreams: ['10.23.0.1'],
    domestic_domain_source: 'domains-min.txt',
    domestic_domain_count: 1234,
  },
  editor_defaults: { dns_config: '' },
  configuration_context: {
    key: 'sing-box',
    target: { core: 'sing-box' },
    capabilities: { features: ['dns.remote_upstream', 'dns.fake_ip', 'dns.system_takeover'], protocols: [] },
  },
}

describe('DNS page', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'zh-CN')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const path = new URL(String(input)).pathname
      if (path.endsWith('/dns/settings')) return Response.json(settings)
      if (path.endsWith('/dns/queries')) return Response.json({ queries: [{ time: 1_725_000_000_000, client: '10.23.0.153', name: 'example.com.', type: 'A', decision: 'core', answers: ['example.com. 60 IN A 198.18.0.1'], upstream: '127.0.0.1:1053', latency_ms: 2, detail: 'default-remote' }] })
      return Response.json({}, { status: 404 })
    }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('shows independent query, rewrite, and settings workspaces', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Dns /></SessionProvider></I18nProvider></QueryClientProvider>)

    expect(await screen.findByText('设备级 DNS 策略；切换订阅和路由规则不会改变这里的配置。')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '查询日志' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'DNS 重写' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '设置与状态' })).toBeInTheDocument()
    expect(await screen.findByText('example.com.')).toBeInTheDocument()
    expect(screen.getByText('10.23.0.153')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '设置与状态' }))
    expect(screen.getByText('fake-ip')).toBeInTheDocument()
    expect(screen.getByDisplayValue('8.8.8.8')).toBeInTheDocument()
  })
})
