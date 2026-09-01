import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { createRef, type Ref } from 'react'
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
  groups: [],
  rules: [],
  rule_providers: [],
  filters: [],
	core_overrides: {},
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
			'native_override', 'private_access', 'inbound.local_proxy', 'transparent.tun', 'transparent.tun.address',
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
  dns_config: '',
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

  it('keeps the complete Toolbox editor while omitting multi-user controls', async () => {
    localStorage.setItem('sempre.locale', 'en')
    const rendered = renderEditor()

    const labels = ['Basic', 'Subscribe URL', 'Rule List', 'Proxy Groups', 'Custom Rules', 'DNS Config', 'Private Access', 'Runtime', 'Manual Servers', 'Diagnostics']
    for (const label of labels) {
      expect(await screen.findByRole('button', { name: label })).toBeInTheDocument()
    }
    expect(labels.map((label) => screen.getByRole('button', { name: label }).textContent)).toEqual(labels)
    expect(within(screen.getByRole('button', { name: 'Basic' })).getByText('Basic')).toHaveClass('text-sm', 'font-medium')
    expect(within(screen.getByRole('button', { name: 'Subscribe URL' })).getByText('Subscribe URL')).toHaveClass('text-sm', 'font-normal')
    expect(screen.queryByText('Authorized Users')).not.toBeInTheDocument()
    expect(screen.getByText('Update schedule')).toBeInTheDocument()
    expect(screen.getByText('Restart after scheduled updates')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Cancel' })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Subscribe URL' }))
    expect(within(screen.getByRole('button', { name: 'Basic' })).getByText('Basic')).toHaveClass('text-sm', 'font-normal')
    expect(within(screen.getByRole('button', { name: 'Subscribe URL' })).getByText('Subscribe URL')).toHaveClass('text-sm', 'font-medium')
    expect(screen.getByRole('button', { name: 'Add Subscribe Source' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Add Raw Source' })).toBeInTheDocument()
    expect(screen.getByText('Node Filter')).toBeInTheDocument()
    expect(rendered.container.querySelector('svg.lucide-circle-play')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Private Access' }))
    expect(screen.getByText('Add connector')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Manual Servers' }))
    expect(screen.getByText('Configuration nodes')).toBeInTheDocument()
    expect(screen.getAllByText('Manual Servers').length).toBeGreaterThan(1)

	  fireEvent.click(screen.getByRole('button', { name: 'Diagnostics' }))
	  expect(screen.getByText('Diagnostic tools')).toBeInTheDocument()
	})

	it('does not expose runtime credentials as browser login fields while saving rules', async () => {
		vi.useFakeTimers()
		localStorage.setItem('sempre.locale', 'en')
		const { container, onSave } = renderEditor()
		expect(container.querySelector('form')).toHaveAttribute('autocomplete', 'off')
		expect(container.querySelector('input[type="password"]')).not.toBeInTheDocument()
		fireEvent.click(screen.getByRole('button', { name: 'Runtime' }))
		expect(screen.getByLabelText('Local proxy username')).toHaveAttribute('autocomplete', 'off')
		expect(screen.getByLabelText('Local proxy password')).toHaveAttribute('autocomplete', 'off')
		expect(screen.getByLabelText('Fixed secret')).toHaveAttribute('autocomplete', 'off')
		expect(screen.getByLabelText('Local proxy username')).toHaveAttribute('data-bwignore', 'true')
		expect(screen.getByLabelText('Local proxy password')).toHaveAttribute('data-bwignore', 'true')
		expect(screen.getByLabelText('Fixed secret')).toHaveAttribute('data-bwignore', 'true')

		fireEvent.click(screen.getByRole('button', { name: 'Rule List' }))
		expect(screen.queryByLabelText('Local proxy password')).not.toBeInTheDocument()
		fireEvent.click(screen.getAllByRole('checkbox', { name: 'Use Recommended Config' })[0])
		fireEvent.change(screen.getAllByLabelText('JSONC editor')[0], { target: { value: '{"geoip-cn": {}}' } })
		await act(async () => vi.advanceTimersByTime(800))
		expect(onSave).toHaveBeenCalledTimes(1)
		expect(onSave.mock.calls[0][0]).toMatchObject({
			local_proxy: { username: 'sempre', password: 'local-secret' },
			management_api: { secret: 'management-secret' },
			editor: { rule_list: '{"geoip-cn": {}}' },
		})
	})

	it('edits core DNS in the profile without exposing frontend takeover', async () => {
		localStorage.setItem('sempre.locale', 'en')
		const { onSave } = renderEditor({
			profile: {
				...profile,
				use_system_dns: false,
				editor: { ...profile.editor, dns_config: JSON.stringify({ shared: { systemDnsTakeoverEnabled: true } }) },
			},
		})

		fireEvent.click(await screen.findByRole('button', { name: 'DNS Config' }))
		expect(screen.getByText('Remote DNS')).toBeInTheDocument()
		expect(screen.getByText('FakeIP')).toBeInTheDocument()
		expect(screen.queryByText('System DNS takeover')).not.toBeInTheDocument()
		vi.useFakeTimers()
		fireEvent.change(screen.getByDisplayValue('8.8.8.8'), { target: { value: '1.1.1.1' } })
		await act(async () => vi.advanceTimersByTime(800))

		expect(onSave).toHaveBeenCalledTimes(1)
		expect(onSave.mock.calls[0][0].use_system_dns).toBe(false)
		expect(onSave.mock.calls[0][0].editor.dns_config).toContain('1.1.1.1')
	})

	it('starts Subscribe URL empty and allows deleting the last source', async () => {
    vi.useFakeTimers()
    localStorage.setItem('sempre.locale', 'en')
    const { onSave } = renderEditor({ profile: { ...profile, sources: [] } })

    fireEvent.click(screen.getByRole('button', { name: 'Subscribe URL' }))
    expect(screen.queryByPlaceholderText('URL')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Add Subscribe Source' }))
    expect(screen.getByPlaceholderText('URL')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Delete' }))
    await act(async () => vi.advanceTimersByTime(800))

    expect(screen.queryByPlaceholderText('URL')).not.toBeInTheDocument()
    expect(onSave).toHaveBeenCalledTimes(1)
    expect(onSave.mock.calls[0][0].sources).toEqual([])
  })

  it('debounces profile and schedule changes before saving the latest values', async () => {
    vi.useFakeTimers()
    localStorage.setItem('sempre.locale', 'en')
    const { onSave, onScheduleSave } = renderEditor()

    const remark = screen.getByLabelText('Remark')
    fireEvent.change(remark, { target: { value: 'First' } })
    fireEvent.change(remark, { target: { value: 'Latest' } })
    await act(async () => vi.advanceTimersByTime(799))
    expect(onSave).not.toHaveBeenCalled()
    await act(async () => vi.advanceTimersByTime(1))
    expect(onSave).toHaveBeenCalledTimes(1)
    expect(onSave.mock.calls[0][0]).toMatchObject({ remark: 'Latest' })
    expect(screen.getByRole('status')).toHaveTextContent('Saved')

    fireEvent.change(screen.getByLabelText('Update schedule'), { target: { value: '12h' } })
    await act(async () => vi.advanceTimersByTime(800))
    expect(onScheduleSave).toHaveBeenCalledWith({ interval: '12h' })

    fireEvent.click(screen.getByRole('checkbox', { name: 'Restart after scheduled updates' }))
    await act(async () => Promise.resolve())
    expect(onScheduleSave).toHaveBeenCalledWith({ auto_restart: false })
  })

  it('saves a dirty profile immediately through the editor handle', async () => {
    vi.useFakeTimers()
    localStorage.setItem('sempre.locale', 'en')
    const editorRef = createRef<ProxySubscribeEditorRef>()
    const onSaveStateChange = vi.fn()
    const { onSave } = renderEditor({ editorRef, onSaveStateChange })

    expect(onSaveStateChange).toHaveBeenLastCalledWith({ profileID: 'profile-1', dirty: false, saving: false })
    fireEvent.change(screen.getByLabelText('Remark'), { target: { value: 'Save now' } })
    expect(onSaveStateChange).toHaveBeenLastCalledWith({ profileID: 'profile-1', dirty: true, saving: false })

    act(() => editorRef.current?.saveNow())
    await act(async () => Promise.resolve())

    expect(onSave).toHaveBeenCalledTimes(1)
    expect(onSave.mock.calls[0][0]).toMatchObject({ remark: 'Save now' })
    expect(onSaveStateChange).toHaveBeenLastCalledWith({ profileID: 'profile-1', dirty: false, saving: false })
  })

  it('reports loading during autosave and becomes clean after it succeeds', async () => {
    vi.useFakeTimers()
    localStorage.setItem('sempre.locale', 'en')
    let resolveSave: (() => void) | undefined
    const pendingSave = new Promise<void>((resolve) => { resolveSave = resolve })
    const onSaveStateChange = vi.fn()
    renderEditor({ onSave: () => pendingSave, onSaveStateChange })

    fireEvent.change(screen.getByLabelText('Remark'), { target: { value: 'Autosaving' } })
    await act(async () => vi.advanceTimersByTime(800))
    expect(onSaveStateChange).toHaveBeenLastCalledWith({ profileID: 'profile-1', dirty: true, saving: true })

    await act(async () => resolveSave?.())
    expect(onSaveStateChange).toHaveBeenLastCalledWith({ profileID: 'profile-1', dirty: false, saving: false })
  })

	it('persists Linux TProxy and directly available authenticated management settings', async () => {
		vi.useFakeTimers()
		localStorage.setItem('sempre.locale', 'en')
		const { onSave } = renderEditor()
		fireEvent.click(screen.getByRole('button', { name: 'Runtime' }))
		fireEvent.click(screen.getByText('TUN Router'))
		fireEvent.click(screen.getByText('TProxy'))
		expect(screen.queryByText('vmbr1')).not.toBeInTheDocument()

		expect(screen.getByText(/Use a strong secret/i)).toBeInTheDocument()
		fireEvent.change(screen.getByLabelText(/Fixed secret/i), { target: { value: 'fixed-secret' } })
		await act(async () => vi.advanceTimersByTime(800))

		expect(onSave).toHaveBeenCalledTimes(1)
		expect(onSave.mock.calls[0][0]).toMatchObject({
			local_proxy: { socks_port: 1080, http_port: 1081, username: 'sempre', password: 'local-secret' },
			transparent_proxy: {
				mode: 'tproxy',
				capture_host: false,
				lan_interfaces: [],
				tproxy: { listen_port: 7893, dns_listen_port: 1053 },
			},
			management_api: { external_controller: '0.0.0.0:9090', secret: 'fixed-secret' },
		})
	})

	it('saves an invalid Linux TUN interface draft for runtime validation later', async () => {
		vi.useFakeTimers()
		localStorage.setItem('sempre.locale', 'en')
		const { onSave } = renderEditor()
		fireEvent.click(screen.getByRole('button', { name: 'Runtime' }))

		fireEvent.change(screen.getByLabelText('TUN interface'), { target: { value: '   ' } })
		await act(async () => vi.advanceTimersByTime(800))
		expect(onSave).toHaveBeenCalledTimes(1)
		expect(onSave.mock.calls[0][0]).toMatchObject({
			transparent_proxy: { tun: { interface_name: '   ' } },
		})
	})

  it('serializes saves and submits only the newest queued profile', async () => {
    vi.useFakeTimers()
    localStorage.setItem('sempre.locale', 'en')
    let resolveFirst: (() => void) | undefined
    const firstSave = new Promise<void>((resolve) => { resolveFirst = resolve })
    const { onSave } = renderEditor({ onSave: vi.fn().mockReturnValueOnce(firstSave).mockResolvedValue(undefined) })

    fireEvent.change(screen.getByLabelText('Remark'), { target: { value: 'First' } })
    await act(async () => vi.advanceTimersByTime(800))
    expect(onSave).toHaveBeenCalledTimes(1)
    fireEvent.change(screen.getByLabelText('Remark'), { target: { value: 'Newest' } })
    await act(async () => vi.advanceTimersByTime(800))
    expect(onSave).toHaveBeenCalledTimes(1)

    await act(async () => resolveFirst?.())
    expect(onSave).toHaveBeenCalledTimes(2)
    expect(onSave.mock.calls[1][0]).toMatchObject({ remark: 'Newest' })
  })

  it('does not expose JSON overrides and preserves legacy override data on save', async () => {
		vi.useFakeTimers()
		localStorage.setItem('sempre.locale', 'en')
		const { onSave } = renderEditor({ profile: { ...profile, core_overrides: { 'sing-box': { route: { final: 'proxy' } } } } })
		expect(screen.queryByRole('button', { name: 'Advanced Config' })).not.toBeInTheDocument()

		fireEvent.change(screen.getByLabelText('Remark'), { target: { value: 'Updated' } })
		await act(async () => vi.advanceTimersByTime(800))
		expect(onSave).toHaveBeenCalledTimes(1)
		expect(onSave.mock.calls[0][0]).toMatchObject({ core_overrides: { 'sing-box': { route: { final: 'proxy' } } } })
  })

  it('saves temporarily invalid Custom Rules for runtime validation later', async () => {
    vi.useFakeTimers()
    localStorage.setItem('sempre.locale', 'en')
    const { onSave } = renderEditor()
    fireEvent.click(screen.getByRole('button', { name: 'Custom Rules' }))
    fireEvent.click(screen.getAllByRole('checkbox', { name: 'Use Recommended Config' })[2])

    const customRules = screen.getAllByLabelText('JSONC editor')[2]
    fireEvent.change(customRules, { target: { value: '{ temporarily invalid JSONC' } })
    await act(async () => vi.advanceTimersByTime(800))
		expect(onSave).toHaveBeenCalledTimes(1)
		expect(onSave.mock.calls[0][0]).toMatchObject({
		  editor: { custom_config: '{ temporarily invalid JSONC' },
		  core_overrides: {},
		})
	  })

	it('shows only common settings when no core is selected', async () => {
		localStorage.setItem('sempre.locale', 'en')
		renderEditor({
			configurationContext: {
				key: 'common', platform: 'linux',
				capabilities: {
					features: [
						'logging.level',
						'dns.local_upstream', 'dns.remote_upstream', 'routing.rules', 'routing.rule_providers',
						'routing.selector', 'transparent.tun', 'management.external_api',
					],
					enum_values: {},
					protocols: [{ protocol: 'trojan', transports: ['tcp'], security: ['tls'] }],
				},
			},
		})
		expect(await screen.findByRole('button', { name: 'DNS Config' })).toBeInTheDocument()
		expect(screen.getByRole('button', { name: 'Runtime' })).toBeInTheDocument()
		expect(screen.getByRole('button', { name: 'Rule List' })).toBeInTheDocument()
		expect(screen.getByRole('button', { name: 'Proxy Groups' })).toBeInTheDocument()
		expect(screen.getByRole('button', { name: 'Manual Servers' })).toBeInTheDocument()
		expect(screen.queryByRole('button', { name: 'Advanced Config' })).not.toBeInTheDocument()
		expect(screen.queryByRole('button', { name: 'Private Access' })).not.toBeInTheDocument()
	})

	it('hides settings that are absent from the selected core capability contract', async () => {
		localStorage.setItem('sempre.locale', 'en')
		renderEditor({
			configurationContext: {
				key: 'limited', platform: 'linux',
				target: { core: 'limited', version: '1.0.0', compiler_target: { core: 'limited', format: 'limited' }, key: 'limited' },
				capabilities: { features: ['transparent.tun'], enum_values: {}, protocols: [] },
			},
		})
		expect(await screen.findByRole('button', { name: 'Runtime' })).toBeInTheDocument()
		for (const label of ['Rule List', 'Proxy Groups', 'Custom Rules', 'Advanced Config', 'DNS Config', 'Private Access', 'Manual Servers']) {
			expect(screen.queryByRole('button', { name: label })).not.toBeInTheDocument()
		}
		expect(screen.queryByLabelText('Log Level')).not.toBeInTheDocument()
	})

	it('shows and persists only dae eBPF runtime settings', async () => {
		vi.useFakeTimers()
		localStorage.setItem('sempre.locale', 'en')
		const { onSave } = renderEditor({
			configurationContext: {
				key: 'dae', platform: 'linux',
				target: { core: 'dae', version: '2.0.0', compiler_target: { core: 'dae', format: 'dae' }, key: 'dae' },
				capabilities: {
					features: ['logging.level', 'dns.local_upstream', 'dns.remote_upstream', 'routing.rules', 'routing.selector', 'transparent.ebpf', 'transparent.interface_policy', 'native_override'],
					enum_values: {}, protocols: [{ protocol: 'trojan', transports: ['tcp'], security: ['tls'] }],
				},
			},
		})
		fireEvent.click(screen.getByRole('button', { name: 'Runtime' }))
		const modeLabel = screen.getByText('Linux transparent proxy')
		const modeControl = modeLabel.parentElement?.nextElementSibling?.firstElementChild
		expect(modeControl).toBeTruthy()
		fireEvent.click(modeControl!)
		fireEvent.click(screen.getByText('eBPF Router'))
		expect(screen.getByText('WAN interface')).toBeInTheDocument()
		expect(screen.queryByText('LAN interfaces')).not.toBeInTheDocument()
		expect(screen.getByText('Configure kernel parameters automatically')).toBeInTheDocument()
		expect(screen.queryByLabelText('Local SOCKS port')).not.toBeInTheDocument()
		expect(screen.queryByText('External controller')).not.toBeInTheDocument()

		fireEvent.click(screen.getByRole('switch'))
		await act(async () => vi.advanceTimersByTime(800))
		expect(onSave).toHaveBeenCalledTimes(1)
		expect(onSave.mock.calls[0][0]).toMatchObject({
			transparent_proxy: {
				mode: 'ebpf-router',
				lan_interfaces: [],
				ebpf: { wan_interface: 'auto', auto_config_kernel_parameter: true },
			},
		})
	})

  it('shows save failures inline without discarding the edited value', async () => {
    vi.useFakeTimers()
    localStorage.setItem('sempre.locale', 'en')
    renderEditor({ onSave: async () => { throw new Error('Compiled configuration was rejected') } })

    const remark = screen.getByLabelText('Remark')
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

    fireEvent.change(screen.getByLabelText('Remark'), { target: { value: 'Save before leaving' } })
    expect(onSave).not.toHaveBeenCalled()
    unmount()
    await act(async () => Promise.resolve())

    expect(onSave).toHaveBeenCalledTimes(1)
    expect(onSave.mock.calls[0][0]).toMatchObject({ remark: 'Save before leaving' })
  })
})
