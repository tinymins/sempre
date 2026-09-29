import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { useState } from 'react'
import { afterEach, expect, it, vi } from 'vitest'
import { DnsEditor } from './DnsEditor'
import { SubscriptionConfigEditor } from './SubscriptionConfigEditor'
import { EditorProvider } from './i18n'
import type { EditorDraft } from './model'

vi.mock('@acme/components', async importOriginal => ({
  ...await importOriginal<typeof import('@acme/components')>(),
  CodeEditor: ({ value = '', onChange, readOnly }: { value?: string; onChange?: (value: string) => void; readOnly?: boolean }) => <textarea aria-label="JSONC" value={value} readOnly={readOnly} onChange={event => onChange?.(event.target.value)} />,
}))
afterEach(cleanup)

const defaults = { ruleList: '{}', group: '[{"name":"default"}]', filter: '[]', customConfig: '[]', dnsConfig: '{"shared":{"remoteDns":"8.8.8.8"}}' }
const initial: EditorDraft = {
  ...defaults, group: '[{"name":"old custom value"}]', remark: '', logLevel: 'info',
  useSystemRuleList: true, useSystemGroup: false, useSystemFilter: true, useSystemCustomConfig: true, useSystemDnsConfig: true,
  privateAccessConfig: '', servers: '[]', selectedCustomNodeIds: [], subscribeItems: [],
}

it('keeps one settings value: enabling inheritance clears custom data and disabling it copies the current default', async () => {
  const changes = vi.fn()
  function Example() {
    const [value, setValue] = useState(initial)
    return <EditorProvider locale="en"><SubscriptionConfigEditor value={value} defaults={defaults} nodes={[]} onChange={patch => { changes(patch); setValue(current => ({ ...current, ...patch })) }} /></EditorProvider>
  }
  render(<Example />)
  fireEvent.click(screen.getByRole('button', { name: 'Proxy Groups' }))
  expect(await screen.findByRole('textbox', { name: 'JSONC' })).toHaveValue(initial.group)
  fireEvent.click(screen.getByRole('checkbox', { name: 'Use system defaults' }))
  expect(changes).toHaveBeenLastCalledWith({ useSystemGroup: true, group: null })
  expect(screen.getByRole('textbox', { name: 'JSONC' })).toHaveValue(defaults.group)
  expect(screen.getByRole('textbox', { name: 'JSONC' })).toHaveAttribute('readonly')
  fireEvent.click(screen.getByRole('checkbox', { name: 'Use system defaults' }))
  expect(changes).toHaveBeenLastCalledWith({ useSystemGroup: false, group: defaults.group })
  expect(screen.getByRole('textbox', { name: 'JSONC' })).toHaveValue(defaults.group)
})

it('preserves unsaved edits while switching tabs and exposes no simple-mode or runtime controls by default', async () => {
  function Example() {
    const [value, setValue] = useState(initial)
    return <SubscriptionConfigEditor value={value} defaults={defaults} nodes={[]} onChange={patch => setValue(current => ({ ...current, ...patch }))} />
  }
  render(<Example />)
  fireEvent.click(screen.getByRole('button', { name: 'Proxy Groups' }))
  fireEvent.change(await screen.findByRole('textbox', { name: 'JSONC' }), { target: { value: '[ // unfinished' } })
  fireEvent.click(screen.getByRole('button', { name: 'Basic information' }))
  fireEvent.click(screen.getByRole('button', { name: 'Proxy Groups' }))
  expect(screen.getByRole('textbox', { name: 'JSONC' })).toHaveValue('[ // unfinished')
  expect(screen.queryByRole('button', { name: /simple|runtime/i })).not.toBeInTheDocument()
})

it('cannot disable inheritance before the defaults have loaded', () => {
  render(<SubscriptionConfigEditor value={{ ...initial, useSystemGroup: true }} defaults={null} nodes={[]} onChange={vi.fn()} />)
  fireEvent.click(screen.getByRole('button', { name: 'Proxy Groups' }))
  expect(screen.getByRole('checkbox', { name: 'Use system defaults' })).toBeDisabled()
})

it('keeps malformed structured DNS values available for correction without crashing or rewriting them', () => {
  const onChange = vi.fn()
  render(<DnsEditor value='{"shared":{"systemDnsListenHosts":42}}' features={['dns.system_takeover']} onChange={onChange} />)
  expect(screen.getByRole('alert')).toBeInTheDocument()
  expect(screen.getByRole('switch')).toBeDisabled()
  expect(onChange).not.toHaveBeenCalled()
})
