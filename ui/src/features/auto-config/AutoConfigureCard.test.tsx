import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../../lib/i18n'
import { SessionProvider } from '../../lib/session'
import { AutoConfigureCard } from './AutoConfigureCard'

const recommendation = {
  id: 'sing-box/macos-standalone-v12', core: 'sing-box', reference: 'sing-box@1.12.20',
  configuration_mode: 'macos-tun-real-ip', score: 100,
  reasons: ['macos-standalone-compatible', 'legacy-destination-override'], warnings: ['legacy-core-version'],
  installed: true, selected: true,
}

const report = {
  checked_at: '2026-08-11T00:00:00Z', platform: 'darwin', architecture: 'arm64', recommendation,
  candidates: [recommendation, {
    id: 'sing-box/macos-stable', core: 'sing-box', reference: 'sing-box@stable',
    configuration_mode: 'macos-tun-external-dns', score: 55,
    reasons: ['stable-release'], warnings: ['external-system-dns-required'], installed: false, selected: false,
  }],
  checks: [
    { id: 'platform', status: 'pass', detail: 'darwin/arm64' },
    { id: 'system-dns-boundary', status: 'info', detail: 'Sempre does not modify macOS system DNS' },
  ],
}

describe('AutoConfigureCard', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
    sessionStorage.clear()
  })

  it('shows the diagnosis before applying the selected candidate', async () => {
    const fetch = vi.fn()
      .mockResolvedValueOnce(Response.json(report))
      .mockResolvedValueOnce(Response.json({ recommendation, changes: [] }))
    vi.stubGlobal('fetch', fetch)
    renderCard()

    fireEvent.click(screen.getByRole('button', { name: 'Start smart diagnosis' }))
    expect(await screen.findByText('sing-box@1.12.20')).toBeInTheDocument()
    expect(screen.getByText('macOS TUN · Real IP · destination override')).toBeInTheDocument()
    expect(screen.getByText('sing-box@stable')).toBeInTheDocument()
    expect(fetch).toHaveBeenNthCalledWith(1, 'http://sempre.test/api/v1/cores/auto/diagnose', expect.objectContaining({ method: 'POST' }))

    fireEvent.click(screen.getByRole('button', { name: 'Apply recommendation' }))
    expect(await screen.findByText('Recommendation applied: sing-box@1.12.20')).toBeInTheDocument()
    await waitFor(() => expect(fetch).toHaveBeenCalledTimes(2))
    expect(fetch).toHaveBeenNthCalledWith(2, 'http://sempre.test/api/v1/cores/auto/apply', expect.objectContaining({
      method: 'POST', body: JSON.stringify({ candidate_id: recommendation.id }),
    }))
  })
})

function renderCard() {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })}>
      <I18nProvider><SessionProvider><AutoConfigureCard /></SessionProvider></I18nProvider>
    </QueryClientProvider>,
  )
}
