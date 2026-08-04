import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AcmeContentBoundary } from '@/components/AcmeContentBoundary'
import { I18nProvider } from '@/lib/i18n'
import type { SubscriptionProfile } from '@/lib/types'
import ProxySubscribeModal from './ProxySubscribeModal'

vi.mock('@monaco-editor/react', () => ({
  default: ({ value = '', onChange, options }: { value?: string; onChange?: (value: string) => void; options?: { readOnly?: boolean } }) => (
    <textarea aria-label="JSONC editor" value={value} readOnly={options?.readOnly} onChange={(event) => onChange?.(event.target.value)} />
  ),
  loader: { config: vi.fn() },
}))
vi.mock('monaco-editor', () => ({}))

const profile: SubscriptionProfile = {
  id: 'profile-1',
  name: 'Local subscription',
  remark: 'Home network',
  log_level: 'info',
  editor: {
    rule_list: '{}',
    group: '[]',
    filter: '[]',
    custom_config: '[]',
    dns_config: '',
    private_access_config: '',
    servers: '[]',
  },
  sources: [{ id: 'source-1', type: 'url', enabled: true, url: 'https://example.com/subscription', fetch_mode: 'auto' }],
  custom_node_ids: [],
  groups: [],
  rules: [],
  rule_providers: [],
  filters: [],
  use_system_groups: true,
  use_system_rules: true,
  use_system_filters: true,
  use_system_dns: true,
  use_system_custom_config: true,
  last_runtime_validated: false,
}

const defaults = {
  rule_list: '{}',
  group: '[]',
  filter: '[]',
  custom_config: '[]',
  dns_config: '',
  private_access_config: '',
  servers: '[]',
}

describe('ProxySubscribeModal', () => {
  afterEach(() => cleanup())

  it('keeps the complete Toolbox editor while omitting multi-user controls', async () => {
    localStorage.setItem('sempre.locale', 'en')
    const rendered = render(
      <I18nProvider>
        <AcmeContentBoundary>
          <ProxySubscribeModal
            profile={profile}
            defaults={defaults}
            customNodes={[{ id: 'custom-1', name: 'Local node', proxy: { name: 'Local node', type: 'socks5', server: '127.0.0.1', port: 1080 } }]}
            saving={false}
            onSave={vi.fn()}
            onCancel={vi.fn()}
          />
        </AcmeContentBoundary>
      </I18nProvider>,
    )

    for (const label of ['Basic', 'Subscribe URL', 'Rule List', 'Proxy Groups', 'Custom Config', 'DNS Config', 'Private Access', 'Manual Servers']) {
      expect(await screen.findByRole('button', { name: label })).toBeInTheDocument()
    }
    expect(screen.queryByText('Authorized Users')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Subscribe URL' }))
    expect(screen.getByText('Add Raw Source')).toBeInTheDocument()
    expect(screen.getByText('Node Filter')).toBeInTheDocument()
    expect(rendered.container.querySelector('svg.lucide-circle-play')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Private Access' }))
    expect(screen.getByText('Add connector')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Manual Servers' }))
    expect(screen.getByText('Configuration nodes')).toBeInTheDocument()
    expect(screen.getAllByText('Manual Servers').length).toBeGreaterThan(1)
  })
})
