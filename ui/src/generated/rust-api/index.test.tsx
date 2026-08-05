import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { SessionProvider } from '@/lib/session'
import { proxyApi } from './index'

describe('proxyApi.traceNode', () => {
  beforeEach(() => {
    sessionStorage.clear()
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  })

  afterEach(() => vi.unstubAllGlobals())

  it('does not build or run a request before a node is selected', async () => {
    const fetch = vi.fn()
    vi.stubGlobal('fetch', fetch)
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })

    const { result } = renderHook(() => proxyApi.traceNode.useQuery(undefined), {
      wrapper: ({ children }) => (
        <QueryClientProvider client={client}>
          <SessionProvider>{children}</SessionProvider>
        </QueryClientProvider>
      ),
    })

    await waitFor(() => expect(result.current.fetchStatus).toBe('idle'))
    expect(fetch).not.toHaveBeenCalled()
  })
})
