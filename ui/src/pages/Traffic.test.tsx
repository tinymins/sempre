import { cleanup, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { Traffic } from './Traffic'

vi.mock('../components/RuntimeChart', () => ({ RuntimeChart: () => <div data-testid="runtime-chart" /> }))

describe('Traffic', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
    sessionStorage.clear()
  })

  it('renders history and rotation settings returned by the backend', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.includes('/runtime/events')) return new Response('')
      return Response.json({
        settings: { retention_hours: 72, max_bytes: 64 * 1024 * 1024 },
        storage_bytes: 1024,
        totals: [{ label: 'example.com', download: 2048, upload: 1024 }],
      })
    }))
    renderTraffic()

    expect(await screen.findByText('example.com')).toBeInTheDocument()
    expect(screen.getByText('Backend history storage')).toBeInTheDocument()
    expect(screen.getByText('1.0 KiB / 64.0 MiB')).toBeInTheDocument()
    expect(screen.getByText('3 days')).toBeInTheDocument()
  })
})

function renderTraffic() {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <I18nProvider>
        <SessionProvider>
          <Traffic />
        </SessionProvider>
      </I18nProvider>
    </QueryClientProvider>,
  )
}
