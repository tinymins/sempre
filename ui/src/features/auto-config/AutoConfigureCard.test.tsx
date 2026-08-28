import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../../lib/i18n'
import { SessionProvider } from '../../lib/session'
import { AutoConfigureCard } from './AutoConfigureCard'

const recommendation = {
  id: 'sing-box/macos-native-dns-v14', core: 'sing-box', reference: 'sing-box@1.14.0-beta.13',
  configuration_mode: 'macos-tun-native-dns', eligible: true, score: 70,
  score_breakdown: [
    { id: 'platform', points: 15, maximum: 30 },
    { id: 'release', points: 10, maximum: 25 },
    { id: 'dns', points: 30, maximum: 30 },
    { id: 'protocols', points: 15, maximum: 15 },
  ],
  matched_requirements: ['feature:transparent.tun', 'feature:private_access', 'feature:dns.tun_capture'],
  reasons: ['platform-compatible', 'native-dns-integration', 'requirements-evaluated'],
  warnings: ['preview-release', 'not-fully-verified'], blockers: [], installed: true, selected: false,
}

const report = {
  checked_at: '2026-08-11T00:00:00Z', platform: 'darwin', architecture: 'arm64', policy_version: 'constraint-utility-v1',
  requirements: { required_features: ['dns.tun_capture', 'private_access', 'transparent.tun'], required_protocols: [] }, recommendation,
  candidates: [recommendation, {
    id: 'sing-box/macos-stable', core: 'sing-box', reference: 'sing-box@stable',
    configuration_mode: 'macos-tun-external-dns', eligible: false, score: null, score_breakdown: [],
    matched_requirements: ['feature:transparent.tun'],
    reasons: ['platform-verified', 'stable-release'], warnings: ['external-system-dns-required'],
    blockers: ['missing-feature:dns.tun_capture', 'missing-feature:private_access'], installed: true, selected: true,
  }],
  checks: [
    { id: 'platform', status: 'pass', detail: 'darwin/arm64' },
    { id: 'dns-requirement', status: 'pass', detail: 'active profile requires native macOS DNS integration' },
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
    expect(await screen.findByText('sing-box@1.14.0-beta.13')).toBeInTheDocument()
    expect(screen.getByText('constraint-utility-v1')).toBeInTheDocument()
    expect(screen.getByText('macOS TUN · native DNS integration')).toBeInTheDocument()
    expect(screen.getByText('DNS integrity')).toBeInTheDocument()
    expect(screen.getByText('30/30')).toBeInTheDocument()
    expect(screen.getByText('sing-box@stable')).toBeInTheDocument()
    expect(screen.getByText('Not applicable')).toBeInTheDocument()
    expect(screen.getByText('Missing required capability: TUN DNS capture')).toBeInTheDocument()
    expect(screen.getByText('Missing required capability: Private access')).toBeInTheDocument()
    expect(fetch).toHaveBeenNthCalledWith(1, 'http://sempre.test/api/v1/cores/auto/diagnose', expect.objectContaining({ method: 'POST' }))

    fireEvent.click(screen.getByRole('button', { name: 'Apply recommendation' }))
    expect(await screen.findByText('Recommendation applied: sing-box@1.14.0-beta.13')).toBeInTheDocument()
    await waitFor(() => expect(fetch).toHaveBeenCalledTimes(2))
    expect(fetch).toHaveBeenNthCalledWith(2, 'http://sempre.test/api/v1/cores/auto/apply', expect.objectContaining({
      method: 'POST', body: JSON.stringify({ candidate_id: recommendation.id }),
    }))
  })

  it('shows every rejected candidate when no recommendation is eligible', async () => {
    const rejected = {
      ...report,
      recommendation: undefined,
      candidates: report.candidates.map((candidate) => ({ ...candidate, eligible: false, score: null, score_breakdown: [] })),
    }
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json(rejected)))
    renderCard()

    fireEvent.click(screen.getByRole('button', { name: 'Start smart diagnosis' }))
    expect(await screen.findByText('No automatic configuration candidate satisfies the active profile.')).toBeInTheDocument()
    expect(screen.getByText('sing-box@1.14.0-beta.13')).toBeInTheDocument()
    expect(screen.getByText('sing-box@stable')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Apply recommendation' })).toBeDisabled()
  })
})

function renderCard() {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })}>
      <I18nProvider><SessionProvider><AutoConfigureCard /></SessionProvider></I18nProvider>
    </QueryClientProvider>,
  )
}
