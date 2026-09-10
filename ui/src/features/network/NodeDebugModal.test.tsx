import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../../lib/i18n'
import { SessionProvider } from '../../lib/session'
import { NodeDebugModal } from './NodeDebugModal'

describe('NodeDebugModal', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
    sessionStorage.clear()
  })

  it('shows exit IP, ASN, location, network, and probe source', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => sseResponse([
      ['step', { id: 'domestic-ip', label: '国内出口 IP', state: 'succeeded', duration_ms: 83, data: {
        url: 'https://ip.3322.net', status: 200, ip: '183.131.177.101', metadata: {
          country_code: 'CN', country: 'China', region: 'Zhejiang', city: 'Hangzhou', asn: 4134, isp: 'China Telecom',
        },
      } }],
      ['step', { id: 'foreign-ip', label: '国外出口 IP', state: 'succeeded', duration_ms: 136, data: {
        url: 'https://api64.ipify.org?format=json', status: 200, ip: '144.34.229.119', metadata: {
          country_code: 'US', country: 'United States', region: 'California', city: 'Los Angeles', asn: 13335, asn_organization: 'Cloudflare, Inc.',
        },
      } }],
      ['done', { node: 'Tokyo 01', duration_ms: 219 }],
    ])))

    render(<I18nProvider><SessionProvider><NodeDebugModal node="Tokyo 01" open onClose={() => undefined} /></SessionProvider></I18nProvider>)

    expect(await screen.findByText('183.131.177.101')).toBeInTheDocument()
    expect(screen.getByText('AS4134')).toBeInTheDocument()
    expect(screen.getByText('🇨🇳 China · Zhejiang · Hangzhou')).toBeInTheDocument()
    expect(screen.getByText('China Telecom')).toBeInTheDocument()
    expect(screen.getByText(/Source · ip\.3322\.net/)).toBeInTheDocument()
    expect(screen.getByText('144.34.229.119')).toBeInTheDocument()
    expect(screen.getByText('AS13335')).toBeInTheDocument()
    expect(screen.getByText('🇺🇸 United States · California · Los Angeles')).toBeInTheDocument()
    expect(screen.getByText('Cloudflare, Inc.')).toBeInTheDocument()
    expect(screen.getByText(/Source · api64\.ipify\.org/)).toBeInTheDocument()
    expect(screen.getByText('Completed')).toBeInTheDocument()
  })
})

function sseResponse(events: Array<[string, object]>) {
  const body = events.map(([event, data]) => `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`).join('')
  return new Response(body, { headers: { 'Content-Type': 'text/event-stream' } })
}
