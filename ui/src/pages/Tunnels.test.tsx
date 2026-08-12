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
})
