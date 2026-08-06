import { cleanup, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { Connections } from './Connections'

describe('Connections', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
    sessionStorage.clear()
  })

  it('treats a null connection list as empty', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => Response.json({ download_total: 0, upload_total: 0, connections: null })))
    renderConnections()

    expect(await screen.findByRole('heading', { name: 'Connections' })).toBeInTheDocument()
    expect(screen.getByText('0 · ↓ 0 B · ↑ 0 B')).toBeInTheDocument()
    expect(await screen.findByText('No data')).toBeInTheDocument()
  })
})

function renderConnections() {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <I18nProvider>
        <SessionProvider>
          <Connections />
        </SessionProvider>
      </I18nProvider>
    </QueryClientProvider>,
  )
}
