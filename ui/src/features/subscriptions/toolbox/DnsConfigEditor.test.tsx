import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '@/lib/i18n'
import DnsConfigEditor from './DnsConfigEditor'

vi.mock('@monaco-editor/react', () => ({
  default: ({ value = '', onChange }: { value?: string; onChange?: (value: string) => void }) => (
    <textarea aria-label="JSONC editor" value={value} onChange={(event) => onChange?.(event.target.value)} />
  ),
  loader: { config: vi.fn() },
}))
vi.mock('monaco-editor', () => ({}))

describe('DnsConfigEditor', () => {
  afterEach(() => cleanup())

  it('persists Linux system DNS takeover settings', () => {
    localStorage.setItem('sempre.locale', 'en')
    const onChange = vi.fn()

    render(
      <I18nProvider>
        <DnsConfigEditor
          defaults={JSON.stringify({ shared: { systemDnsListenHosts: ['127.0.0.1'], cnDomainRuleSetUrl: 'https://cdn.jsdelivr.net/gh/SagerNet/sing-geosite@rule-set/geosite-cn.srs' } })}
          features={['dns.local_upstream', 'dns.system_takeover']}
          onChange={onChange}
        />
      </I18nProvider>,
    )

    expect(screen.getByText('Sempre DNS frontend / system takeover')).toBeInTheDocument()
    expect(screen.getByText('Enable DNS frontend')).toBeInTheDocument()
    expect(screen.getByText('Listen addresses')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('switch'))

    expect(onChange).toHaveBeenCalledWith(JSON.stringify({
      shared: { systemDnsTakeoverEnabled: true },
    }, null, 2))
  })

  it('persists selected system DNS listen hosts and makes wildcard exclusive', () => {
    localStorage.setItem('sempre.locale', 'en')
    const onChange = vi.fn()

    render(
      <I18nProvider>
        <DnsConfigEditor
          defaults={JSON.stringify({ shared: { systemDnsListenHosts: ['127.0.0.1'], cnDomainRuleSetUrl: 'https://cdn.jsdelivr.net/gh/SagerNet/sing-geosite@rule-set/geosite-cn.srs' } })}
          features={['dns.system_takeover']}
          value={JSON.stringify({ shared: { systemDnsTakeoverEnabled: true } })}
          systemDnsListenHostOptions={[{ value: '10.10.10.1', label: '10.10.10.1 · vmbr1' }]}
          onChange={onChange}
        />
      </I18nProvider>,
    )

    fireEvent.click(screen.getByText('10.10.10.1 · vmbr1'))
    expect(JSON.parse(onChange.mock.calls.at(-1)?.[0] as string)).toEqual({
      shared: { systemDnsTakeoverEnabled: true, systemDnsListenHosts: ['127.0.0.1', '10.10.10.1'] },
    })

    fireEvent.click(screen.getByText('0.0.0.0'))
    expect(JSON.parse(onChange.mock.calls.at(-1)?.[0] as string)).toEqual({
      shared: { systemDnsTakeoverEnabled: true, systemDnsListenHosts: ['0.0.0.0'] },
    })
  })

  it('edits managed GEO sources and preserves fields without controls', () => {
    localStorage.setItem('sempre.locale', 'en')
    const onChange = vi.fn()

    render(
      <I18nProvider>
        <DnsConfigEditor
          defaults={JSON.stringify({ shared: { systemDnsListenHosts: ['127.0.0.1'], cnDomainRuleSetUrl: 'https://cdn.jsdelivr.net/gh/SagerNet/sing-geosite@rule-set/geosite-cn.srs' } })}
          features={['dns.local_upstream', 'dns.local_transport', 'dns.geo_sources']}
          value={JSON.stringify({
            shared: { managedDnsFrontend: true, unknown: 'hidden' },
          })}
          onChange={onChange}
        />
      </I18nProvider>,
    )

    expect(screen.getByText('GEO Rule Sets')).toBeInTheDocument()
    const cnDomainUrl = screen.getByDisplayValue('https://cdn.jsdelivr.net/gh/SagerNet/sing-geosite@rule-set/geosite-cn.srs')
    fireEvent.change(cnDomainUrl, { target: { value: 'https://rules.example/geosite-cn.srs' } })

    const saved = JSON.parse(onChange.mock.calls.at(-1)?.[0] as string)
    expect(saved).toEqual({ shared: { managedDnsFrontend: true, unknown: 'hidden', cnDomainRuleSetUrl: 'https://rules.example/geosite-cn.srs' } })
    expect(saved.shared.managedDnsFrontend).toBe(true)
    expect(saved.shared.unknown).toBe('hidden')
  })
})
