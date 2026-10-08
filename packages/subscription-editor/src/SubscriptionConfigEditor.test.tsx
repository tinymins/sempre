import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { useState } from 'react'
import { afterEach, expect, it, vi } from 'vitest'
import { DnsEditor } from './DnsEditor'
import { SubscriptionConfigEditor } from './SubscriptionConfigEditor'
import { EditorProvider } from './i18n'
import type { EditorDraft } from './model'

vi.mock('@acme/components', async importOriginal => ({
  ...await importOriginal<typeof import('@acme/components')>(),
  CodeEditor: ({ value = '', onChange, readOnly, appearance, height }: { value?: string; onChange?: (value: string) => void; readOnly?: boolean; appearance?: string; height?: string | number }) => <textarea aria-label="JSONC" data-appearance={appearance} data-height={height} value={value} readOnly={readOnly} onChange={event => onChange?.(event.target.value)} />,
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
  function Example({ readOnly = false }: { readOnly?: boolean }) {
    const [value, setValue] = useState(initial)
    return <SubscriptionConfigEditor layout="page" readOnly={readOnly} tabBarFooter={<div data-testid="save-status" />} value={value} defaults={defaults} nodes={[]} onChange={patch => setValue(current => ({ ...current, ...patch }))} />
  }
  const { rerender } = render(<Example />)
  fireEvent.click(screen.getByRole('button', { name: 'Proxy Groups' }))
  expect(screen.getByRole('button', { name: 'Proxy Groups' }).querySelector('span.text-sm')).toHaveClass('font-medium')
  expect(screen.getByRole('button', { name: 'Basic information' }).querySelector('span.text-sm')).toHaveClass('font-normal')
  expect(screen.getByTestId('save-status').previousElementSibling).toHaveClass('mb-3', 'pb-1')
  expect(await screen.findByRole('textbox', { name: 'JSONC' })).toHaveAttribute('data-appearance', 'plain')
  expect(screen.getByRole('textbox', { name: 'JSONC' })).toHaveAttribute('data-height', 'calc(100vh - 280px)')
  fireEvent.change(await screen.findByRole('textbox', { name: 'JSONC' }), { target: { value: '[ // unfinished' } })
  fireEvent.click(screen.getByRole('button', { name: 'Basic information' }))
  fireEvent.click(screen.getByRole('button', { name: 'Proxy Groups' }))
  expect(screen.getByRole('textbox', { name: 'JSONC' })).toHaveValue('[ // unfinished')
  expect(screen.queryByRole('button', { name: /simple|runtime/i })).not.toBeInTheDocument()
  rerender(<Example readOnly />)
  expect(screen.getByRole('button', { name: 'Basic information' })).not.toBeDisabled()
  expect(screen.getByRole('textbox', { name: 'JSONC' })).toHaveAttribute('readonly')
  fireEvent.click(screen.getByRole('button', { name: 'Basic information' }))
  expect(screen.getByRole('textbox', { name: 'Remark' })).toBeDisabled()
  fireEvent.click(screen.getByRole('button', { name: 'Proxy Groups' }))
  expect(screen.getByRole('textbox', { name: 'JSONC' })).toHaveValue('[ // unfinished')
})

it('keeps malformed structured DNS values available for correction without crashing or rewriting them', () => {
  const onChange = vi.fn()
  render(<DnsEditor value='{"shared":{"systemDnsListenHosts":42}}' features={['dns.system_takeover']} onChange={onChange} />)
  expect(screen.getByRole('alert')).toBeInTheDocument()
  expect(screen.getByRole('switch')).toBeDisabled()
  expect(onChange).not.toHaveBeenCalled()
})
