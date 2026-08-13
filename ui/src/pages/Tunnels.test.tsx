import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { SessionProvider } from '../lib/session'
import { I18nProvider } from '../lib/i18n'
import type { TunnelStatus } from '../lib/types'
import { Tunnels } from './Tunnels'

const initialStatus: TunnelStatus = {
  config: { schema: 1, instances: [] },
  binary: { version: '10.5.5', installed: true },
  instances: [],
  forwards: [],
}

describe('Tunnels', () => {
  let bodies: unknown[]
  let sequence: number

  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'zh-CN')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
    bodies = []
    sequence = 0
    vi.stubGlobal('crypto', { randomUUID: () => `0000000${++sequence}-0000-4000-8000-000000000000` })
    vi.stubGlobal('fetch', vi.fn(async (_input: RequestInfo | URL, init: RequestInit = {}) => {
      if ((init.method ?? 'GET') === 'PUT') {
        const config = JSON.parse(String(init.body))
        bodies.push(config)
        return new Response(JSON.stringify({ status: { ...initialStatus, config } }), { status: 200, headers: { 'Content-Type': 'application/json' } })
      }
      return new Response(JSON.stringify(initialStatus), { status: 200, headers: { 'Content-Type': 'application/json' } })
    }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('allocates globally unique forward IDs and listen ports across remote instances', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Tunnels /></SessionProvider></I18nProvider></QueryClientProvider>)

    await screen.findByText('尚未配置隧道。每台远端 OpenWrt 添加一个客户端实例。')
    fireEvent.click(screen.getByRole('button', { name: '新增远端实例' }))
    fireEvent.click(screen.getByRole('button', { name: '新增远端实例' }))
    for (const button of screen.getAllByRole('button', { name: '新增转发' })) fireEvent.click(button)
    fireEvent.click(screen.getByRole('button', { name: '保存' }))

    await waitFor(() => expect(bodies).toHaveLength(1))
    const saved = bodies[0] as TunnelStatus['config']
    expect(saved.instances.map((instance) => instance.id)).toEqual(['tunnel-00000001', 'tunnel-00000002'])
    expect(saved.instances.flatMap((instance) => instance.forwards.map((forward) => forward.id))).toEqual(['wg-00000003', 'wg-00000004'])
    expect(saved.instances.flatMap((instance) => instance.forwards.map((forward) => forward.listen_port))).toEqual([52001, 52002])
  })

  it('hides generated IDs and keeps advanced parameters collapsed by default', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Tunnels /></SessionProvider></I18nProvider></QueryClientProvider>)

    await screen.findByText('尚未配置隧道。每台远端 OpenWrt 添加一个客户端实例。')
    fireEvent.click(screen.getByRole('button', { name: '新增远端实例' }))

    expect(screen.queryByText('实例 ID')).not.toBeInTheDocument()
    expect(screen.queryByDisplayValue('tunnel-00000001')).not.toBeInTheDocument()
    expect(screen.queryByText('DNS 服务器 IP')).not.toBeInTheDocument()
    expect(screen.queryByText('WSS 服务地址')).not.toBeInTheDocument()
    expect(screen.getByText('保持运行')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '高级参数' }))
    expect(screen.getByText('DNS 服务器 IP')).toBeInTheDocument()
    expect(screen.getByText('DNS 类型')).toBeInTheDocument()
    expect(screen.getByDisplayValue('15s')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '新增转发' }))
    expect(screen.queryByText('Forward ID')).not.toBeInTheDocument()
    expect(screen.queryByDisplayValue('wg-00000002')).not.toBeInTheDocument()
    expect(screen.queryByText('远端主机')).not.toBeInTheDocument()

    fireEvent.click(screen.getAllByRole('button', { name: '高级参数' })[1])
    expect(screen.getByText('远端主机')).toBeInTheDocument()
    expect(screen.getByText('UDP 超时秒')).toBeInTheDocument()
  })

  it('builds internal wstunnel endpoint URLs from domain and port fields', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Tunnels /></SessionProvider></I18nProvider></QueryClientProvider>)

    await screen.findByText('尚未配置隧道。每台远端 OpenWrt 添加一个客户端实例。')
    fireEvent.click(screen.getByRole('button', { name: '新增远端实例' }))
    fireEvent.change(screen.getByLabelText('对端域名'), { target: { value: 'hz.example.com' } })
    fireEvent.click(screen.getByRole('button', { name: '高级参数' }))
    fireEvent.click(screen.getByRole('combobox'))
    fireEvent.click(await screen.findByText('DoH'))
    expect(screen.getByLabelText('DNS 端口')).toHaveValue('443')
    fireEvent.click(screen.getByRole('combobox'))
    fireEvent.click(await screen.findByText('DoT'))
    expect(screen.getByLabelText('DNS 端口')).toHaveValue('853')
    fireEvent.click(screen.getByRole('combobox'))
    fireEvent.click(await screen.findByText('DoH'))
    fireEvent.change(screen.getByLabelText('DNS 服务器 IP'), { target: { value: '203.0.113.53' } })
    fireEvent.change(screen.getByLabelText('TLS 服务器名（SNI）'), { target: { value: 'dns.example.com' } })
    fireEvent.click(screen.getByRole('button', { name: '保存' }))

    await waitFor(() => expect(bodies).toHaveLength(1))
    const saved = bodies[0] as TunnelStatus['config']
    expect(saved.instances[0].server_url).toBe('wss://hz.example.com:443')
    expect(saved.instances[0].dns_resolvers).toEqual(['dns+https://203.0.113.53:443?sni=dns.example.com'])
  })
})
