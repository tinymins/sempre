import { cleanup, fireEvent, render, screen } from '@testing-library/react'
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
      ] : []
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

    fireEvent.click(globalGroup)
    expect(screen.queryByText('global-node')).not.toBeInTheDocument()
  })
})
