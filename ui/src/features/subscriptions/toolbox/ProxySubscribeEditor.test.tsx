import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AcmeContentBoundary } from '@/components/AcmeContentBoundary'
import { I18nProvider } from '@/lib/i18n'
import type { SubscriptionProfile } from '@/lib/types'
import ProxySubscribeEditor from './ProxySubscribeEditor'

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

describe('ProxySubscribeEditor', () => {
  afterEach(() => {
    cleanup()
    vi.useRealTimers()
  })

  function renderEditor(overrides: { onSave?: (candidate: SubscriptionProfile) => Promise<void> | void; onScheduleSave?: (change: { interval?: string; auto_restart?: boolean }) => Promise<void> | void } = {}) {
    const onSave = vi.fn(overrides.onSave ?? (() => undefined))
    const onScheduleSave = vi.fn(overrides.onScheduleSave ?? (() => undefined))
    const rendered = render(
      <I18nProvider>
        <AcmeContentBoundary>
          <ProxySubscribeEditor
            profile={profile}
            defaults={defaults}
            customNodes={[{ id: 'custom-1', name: 'Local node', proxy: { name: 'Local node', type: 'socks5', server: '127.0.0.1', port: 1080 } }]}
            schedule={{ interval: '24h', autoRestart: true }}
            onScheduleSave={onScheduleSave}
            onSave={onSave}
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

    const labels = ['Basic', 'Subscribe URL', 'Rule List', 'Proxy Groups', 'Custom Rules', 'Advanced Config', 'DNS Config', 'Private Access', 'Manual Servers', 'Diagnostics']
    for (const label of labels) {
      expect(await screen.findByRole('button', { name: label })).toBeInTheDocument()
    }
    expect(labels.map((label) => screen.getByRole('button', { name: label }).textContent)).toEqual(labels)
    expect(screen.queryByText('Authorized Users')).not.toBeInTheDocument()
    expect(screen.getByText('Update schedule')).toBeInTheDocument()
    expect(screen.getByText('Restart after scheduled updates')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Cancel' })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Subscribe URL' }))
    expect(screen.getByText('Add Raw Source')).toBeInTheDocument()
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

  it('keeps invalid advanced JSON inline and saves it after correction', async () => {
    vi.useFakeTimers()
    localStorage.setItem('sempre.locale', 'en')
    const { onSave } = renderEditor()
    fireEvent.click(screen.getByRole('button', { name: 'Advanced Config' }))

    const advanced = screen.getAllByLabelText('JSONC editor')[3]
    fireEvent.change(advanced, { target: { value: '[]' } })
    await act(async () => vi.advanceTimersByTime(800))
    expect(onSave).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent('Advanced Config must be a JSONC object.')

    fireEvent.change(advanced, { target: { value: '{"route":{"final":"proxy"}}' } })
    await act(async () => vi.advanceTimersByTime(800))
    expect(onSave).toHaveBeenCalledTimes(1)
    expect(onSave.mock.calls[0][0]).toMatchObject({ custom_config: { route: { final: 'proxy' } } })
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  it('validates Custom Rules as a string array and keeps it in the editor field', async () => {
    vi.useFakeTimers()
    localStorage.setItem('sempre.locale', 'en')
    const { onSave } = renderEditor()
    fireEvent.click(screen.getByRole('button', { name: 'Custom Rules' }))
    fireEvent.click(screen.getAllByRole('checkbox', { name: 'Use System Config' })[2])

    const customRules = screen.getAllByLabelText('JSONC editor')[2]
    fireEvent.change(customRules, { target: { value: '{}' } })
    await act(async () => vi.advanceTimersByTime(800))
    expect(onSave).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent('Custom Rules must be a JSONC array of strings.')

    fireEvent.change(customRules, { target: { value: '["domain_suffix:example.com"]' } })
    await act(async () => vi.advanceTimersByTime(800))
    expect(onSave).toHaveBeenCalledTimes(1)
    expect(onSave.mock.calls[0][0]).toMatchObject({
      editor: { custom_config: '["domain_suffix:example.com"]' },
      custom_config: {},
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
