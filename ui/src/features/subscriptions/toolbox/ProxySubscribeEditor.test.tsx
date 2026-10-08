import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { type Ref } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AcmeContentBoundary } from '@/components/AcmeContentBoundary'
import { I18nProvider } from '@/lib/i18n'
import type { SubscriptionConfigurationContext, SubscriptionProfile } from '@/lib/types'
import ProxySubscribeEditor, { type ProxySubscribeEditorRef, type ProxySubscribeSaveState } from './ProxySubscribeEditor'

vi.mock('@monaco-editor/react', () => ({
  default: ({ value = '', onChange, options }: { value?: string; onChange?: (value: string) => void; options?: { readOnly?: boolean } }) => (
    <textarea aria-label="JSONC editor" value={value} readOnly={options?.readOnly} onChange={(event) => onChange?.(event.target.value)} />
  ),
  loader: { config: vi.fn() },
}))
vi.mock('monaco-editor', () => ({}))

const profile: SubscriptionProfile = {
  id: 'profile-1',
	revision: 1,
  name: 'Local subscription',
  mode: 'local',
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
	local_proxy: { socks_port: 1080, http_port: 1081, username: 'sempre', password: 'local-secret' },
	management_api: { external_controller: '0.0.0.0:9090', secret: 'management-secret', allow_origins: [], allow_private_network: false },
  use_system_groups: true,
  use_system_rules: true,
  use_system_filters: true,
  use_system_dns: true,
  use_system_custom_config: true,
  last_runtime_validated: false,
}

const singBoxContext: SubscriptionConfigurationContext = {
	key: 'sing-box-context',
	target: { core: 'sing-box', version: '1.13.16', compiler_target: { core: 'sing-box', format: 'sing-box-v13', version: '13', platform: 'default' }, key: 'sing-box-context' },
	running: { core: 'sing-box', version: '1.13.16' },
	platform: 'linux',
	capabilities: {
		features: [
			'logging.level',
			'dns.local_upstream', 'dns.remote_upstream', 'dns.remote_port', 'dns.bootstrap_upstream', 'dns.bootstrap_port',
			'dns.bootstrap_server_name', 'dns.fake_ip', 'dns.split', 'dns.native', 'dns.prefer_ipv4',
			'dns.remote_server_name', 'dns.remote_detour', 'dns.reject_https', 'dns.system_takeover',
			'routing.rules', 'routing.rule_providers', 'routing.selector', 'routing.url_test',
			'private_access', 'inbound.local_proxy', 'transparent.tun', 'transparent.tun.address',
			'transparent.tproxy', 'transparent.interface_policy', 'management.external_api',
		],
		enum_values: {}, protocols: [{ protocol: 'trojan', transports: ['tcp'], security: ['tls'] }],
	},
}

const defaults = {
  rule_list: '{}',
  group: '[]',
  filter: '[]',
  custom_config: '[]',
  dns_config: JSON.stringify({ shared: { remoteDns: '8.8.8.8' } }),
  private_access_config: '',
  servers: '[]',
}

describe('ProxySubscribeEditor', () => {
  afterEach(() => {
    cleanup()
    vi.useRealTimers()
  })

  function renderEditor(overrides: { profile?: SubscriptionProfile; onSave?: (candidate: SubscriptionProfile) => Promise<void> | void; onScheduleSave?: (change: { interval?: string; auto_restart?: boolean }) => Promise<void> | void; configurationContext?: SubscriptionConfigurationContext; editorRef?: Ref<ProxySubscribeEditorRef>; onSaveStateChange?: (state: ProxySubscribeSaveState) => void } = {}) {
    const onSave = vi.fn(overrides.onSave ?? (() => undefined))
    const onScheduleSave = vi.fn(overrides.onScheduleSave ?? (() => undefined))
    const rendered = render(
      <I18nProvider>
        <AcmeContentBoundary>
          <ProxySubscribeEditor
            ref={overrides.editorRef}
            profile={overrides.profile ?? profile}
            defaults={defaults}
            customNodes={[{ id: 'custom-1', name: 'Local node', proxy: { name: 'Local node', type: 'socks5', server: '127.0.0.1', port: 1080 } }]}
			networkInventory={{
				supported: true,
				default_interface: 'vmbr0',
				recommended_lan_interfaces: ['vmbr1'],
				local_prefixes: ['10.10.10.0/24'],
				vpn_prefixes: [],
				occupied_prefixes: ['10.10.10.0/24'],
				interfaces: [
					{ name: 'vmbr0', index: 2, kind: 'bridge', up: true, default_route: true, addresses: ['10.23.0.200/21'] },
					{ name: 'vmbr1', index: 3, kind: 'bridge', up: true, default_route: false, addresses: ['10.10.10.1/24'] },
				],
			}}
			configurationContext={overrides.configurationContext ?? singBoxContext}
            schedule={{ interval: '24h', autoRestart: true }}
            onScheduleSave={onScheduleSave}
            onSave={onSave}
            onSaveStateChange={overrides.onSaveStateChange}
            diagnostics={<div>Diagnostic tools</div>}
          />
        </AcmeContentBoundary>
      </I18nProvider>,
    )
    return { ...rendered, onSave, onScheduleSave }
  }

  it('serializes saves and submits only the newest queued profile', async () => {
    vi.useFakeTimers()
    localStorage.setItem('sempre.locale', 'en')
    let resolveFirst: (() => void) | undefined
    const firstSave = new Promise<void>((resolve) => { resolveFirst = resolve })
    const sources: SubscriptionProfile['sources'] = [
      { id: 'raw-empty', type: 'raw', enabled: false, content: '', remark: 'Keep empty RAW', snapshot_hash: 'raw-snapshot' },
      { ...profile.sources[0], prefix: 'Home', cache_ttl_minutes: 30, user_agent: 'custom-agent', fetch_mode: 'domestic-direct' },
      { id: 'raw-content', type: 'raw', enabled: true, content: 'proxies: []', prefix: 'RAW' },
    ]
    const editableProfile = { ...profile, sources, custom_node_ids: ['custom-1'], use_system_groups: false, use_system_rules: false, use_system_filters: false, use_system_custom_config: false, use_system_dns: false }
    const { onSave } = renderEditor({ profile: editableProfile, onSave: vi.fn().mockReturnValueOnce(firstSave).mockResolvedValue(undefined) })

    fireEvent.change(screen.getByRole('textbox', { name: 'Remark' }), { target: { value: 'First' } })
    await act(async () => vi.advanceTimersByTime(800))
    expect(onSave).toHaveBeenCalledTimes(1)
    fireEvent.change(screen.getByRole('textbox', { name: 'Remark' }), { target: { value: 'Newest' } })
    await act(async () => vi.advanceTimersByTime(800))
    expect(onSave).toHaveBeenCalledTimes(1)

    await act(async () => resolveFirst?.())
    expect(onSave).toHaveBeenCalledTimes(2)
    expect(onSave.mock.calls[1][0]).toMatchObject({ remark: 'Newest' })
    for (const [candidate] of onSave.mock.calls) {
      expect(candidate.sources).toEqual(sources)
      expect(candidate.custom_node_ids).toEqual(['custom-1'])
      expect(candidate.local_proxy).toEqual(profile.local_proxy)
      expect(candidate.management_api).toEqual(profile.management_api)
      expect(candidate.editor).toEqual(profile.editor)
      expect(candidate.use_system_groups).toBe(editableProfile.use_system_groups)
      expect(candidate.use_system_rules).toBe(editableProfile.use_system_rules)
    }
  })

  it('shows save failures inline without discarding the edited value', async () => {
    vi.useFakeTimers()
    localStorage.setItem('sempre.locale', 'en')
    renderEditor({ onSave: async () => { throw new Error('Compiled configuration was rejected') } })

    const remark = screen.getByRole('textbox', { name: 'Remark' })
    fireEvent.change(remark, { target: { value: 'Unsaved local edit' } })
    await act(async () => vi.advanceTimersByTime(800))
    expect(screen.getByRole('alert')).toHaveTextContent('Compiled configuration was rejected')
    expect(remark).toHaveValue('Unsaved local edit')
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('flushes a pending valid edit when the editor unmounts', async () => {
    vi.useFakeTimers()
    localStorage.setItem('sempre.locale', 'en')
    const { onSave, unmount } = renderEditor()

    fireEvent.change(screen.getByRole('textbox', { name: 'Remark' }), { target: { value: 'Save before leaving' } })
    expect(onSave).not.toHaveBeenCalled()
    unmount()
    await act(async () => Promise.resolve())

    expect(onSave).toHaveBeenCalledTimes(1)
    expect(onSave.mock.calls[0][0]).toMatchObject({ remark: 'Save before leaving' })
  })
})
