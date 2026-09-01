import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { Proxies } from './Proxies'

describe('Proxies', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      const body = url.endsWith('/api/v1/runtime/proxies') ? [
        { name: 'GLOBAL', type: 'Fallback', all: ['active-global', 'global-node'], now: 'active-global' },
        { name: 'configured-second', type: 'Selector', all: ['active-second', 'second-node'], now: 'active-second' },
      ] : url.endsWith('/api/v1/runtime/proxies/delay') ? { delay: 42 } : []
      return new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } })
    }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('keeps API order and starts every proxy group collapsed', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Proxies /></SessionProvider></I18nProvider></QueryClientProvider>)

    const headings = await screen.findAllByRole('heading', { level: 2 })
    expect(headings.map((heading) => heading.textContent)).toEqual(['GLOBAL', 'configured-second'])
    expect(screen.queryByText('global-node')).not.toBeInTheDocument()
    expect(screen.queryByText('second-node')).not.toBeInTheDocument()

    const globalGroup = screen.getByRole('button', { name: /GLOBAL/ })
    fireEvent.click(globalGroup)
    expect(screen.getByText('global-node')).toBeInTheDocument()
    expect(screen.queryByText('second-node')).not.toBeInTheDocument()
    expect(screen.getByRole('radiogroup')).toHaveClass('grid-cols-[repeat(auto-fill,minmax(15rem,1fr))]')
    expect(screen.getByRole('radio', { name: 'global-node' })).toHaveClass('rounded-md', 'border')

    fireEvent.click(globalGroup)
    expect(screen.queryByText('global-node')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Node Providers' })).not.toBeInTheDocument()
    expect(screen.getByText('Proxy groups 2')).toBeInTheDocument()
  })

  it('shows node providers only when the running core exposes them', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.endsWith('/api/v1/runtime/providers')) return Response.json([{ name: 'hong-kong-airport', type: 'Proxy', vehicle_type: 'HTTP', proxies: [{ name: 'HK-01', type: 'ss' }] }])
      if (url.endsWith('/api/v1/runtime/proxies')) return Response.json([{ name: 'GLOBAL', type: 'Selector', all: ['HK-01'], now: 'HK-01' }])
      return Response.json({})
    }))
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Proxies /></SessionProvider></I18nProvider></QueryClientProvider>)

    const providerTab = await screen.findByRole('button', { name: 'Node Providers' })
    expect(screen.getByText('Proxy groups 1 · Node Providers 1')).toBeInTheDocument()
    fireEvent.click(providerTab)
    expect(screen.getByRole('heading', { name: 'hong-kong-airport' })).toBeInTheDocument()
    expect(screen.getByText('HTTP · 1 nodes')).toBeInTheDocument()
    expect(screen.getByText('HK-01')).toBeInTheDocument()
  })

  it('selects a node from the whole row but not from the latency button', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Proxies /></SessionProvider></I18nProvider></QueryClientProvider>)

    fireEvent.click(await screen.findByRole('button', { name: /GLOBAL/ }))
    const activeNode = screen.getByRole('radio', { name: 'active-global' })
    const inactiveNode = screen.getByRole('radio', { name: 'global-node' })
    expect(activeNode).toHaveAttribute('aria-checked', 'true')
    expect(inactiveNode).toHaveAttribute('aria-checked', 'false')

    fireEvent.click(inactiveNode)
    await vi.waitFor(() => expect(fetch).toHaveBeenCalledWith('http://sempre.test/api/v1/runtime/proxies/select', expect.objectContaining({
      method: 'POST',
      body: JSON.stringify({ group: 'GLOBAL', proxy: 'global-node' }),
    })))

    const currentInactiveNode = screen.getByRole('radio', { name: 'global-node' })
    await vi.waitFor(() => expect(within(currentInactiveNode).getByTitle('Test latency')).not.toBeDisabled())
    const selectionCalls = vi.mocked(fetch).mock.calls.filter(([input]) => String(input).endsWith('/api/v1/runtime/proxies/select')).length
    fireEvent.click(within(currentInactiveNode).getByTitle('Test latency'))
    expect(vi.mocked(fetch).mock.calls.filter(([input]) => String(input).endsWith('/api/v1/runtime/proxies/select'))).toHaveLength(selectionCalls)
    await vi.waitFor(() => expect(fetch).toHaveBeenCalledWith('http://sempre.test/api/v1/runtime/proxies/delay', expect.objectContaining({ method: 'POST' })))
  })
})
