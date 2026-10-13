import type { ModalLifecycleProps } from '../use-modal-state'
import { Button, Checkbox, Input, Modal, Select, TextArea } from '@acme/components'
import { useState } from 'react'
import { useI18n } from '../i18n/provider'
import type { CustomNode } from '../subscriptions/types'
import { proxyApi, type ProxyInput } from './api'
import { ManualProxyFields } from './ManualProxyFields'
import { defaultManualContent } from './manual-proxy'

type Mode = 'custom-node' | 'import' | 'manual'
const payload = (input: ProxyInput, mode: Mode): ProxyInput => mode === 'custom-node'
  ? { ...input, name: input.name.trim(), content: undefined }
  : { ...input, name: input.name.trim(), customNodeId: null }
const same = (a: ProxyInput, b: ProxyInput) => JSON.stringify(a) === JSON.stringify(b)

export function ProxyEditor({ open, afterOpenChange, id, initial, nodes, onSaved, onClose }: {
  id: string | null; initial: ProxyInput; nodes: CustomNode[]; onSaved: () => Promise<void>; onClose: () => void
} & ModalLifecycleProps) {
  const { t } = useI18n()
  const initialMode = initial.customNodeId || !id ? 'custom-node' : 'import'
  const [input, setInput] = useState(initial)
  const [mode, setMode] = useState<Mode>(initialMode)
  const original = payload(initial, initialMode)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const patch = (patch: Partial<ProxyInput>) => setInput(current => ({ ...current, ...patch }))
  const save = async () => {
    if (saving) return
    const next = payload(input, mode)
    if (!next.name || (!next.customNodeId && !next.content?.trim())) {
      setError(t(!next.name ? 'proxies.namePlaceholder' : 'proxies.selectCustomNode'))
      return
    }
    if (id && same(next, original)) { onClose(); return }
    const configUnchanged = next.engine === original.engine && next.content === original.content && next.customNodeId === original.customNodeId
    const request = id && configUnchanged ? { name: next.name, engine: next.engine, enabled: next.enabled } : next
    setSaving(true); setError('')
    try { await proxyApi.save(id, request); await onSaved(); onClose() }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setSaving(false) }
  }
  return <Modal open={open} afterOpenChange={afterOpenChange} title={t(id ? 'proxies.edit' : 'proxies.add')} onCancel={() => { if (!saving) onClose() }} closable={!saving} keyboard={!saving} footer={<div className="flex justify-end gap-2">
    <Button disabled={saving} onClick={onClose}>{t('common.cancel')}</Button>
    <Button variant="primary" loading={saving} onClick={() => void save()}>{t(id ? 'common.save' : 'proxies.add')}</Button>
  </div>}>
    <div className="space-y-4">
      {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
      <label className="block space-y-1 text-sm">{t('proxies.name')}<Input disabled={saving} value={input.name} onChange={event => patch({ name: event.target.value })} placeholder={t('proxies.namePlaceholder')} /></label>
      <label className="block space-y-1 text-sm">{t('proxies.engine')}<Select disabled={saving} value={input.engine ?? 'auto'} options={[{ value: 'auto', label: t('proxies.engineAuto') }, { value: 'sing-box', label: 'sing-box' }, { value: 'xray', label: 'Xray' }]} onChange={value => patch({ engine: value === 'auto' ? null : value as ProxyInput['engine'] })} className="w-full" /></label>
      <label className="block space-y-1 text-sm">{t('proxies.inputMode')}<Select disabled={saving} value={mode} options={[{ value: 'custom-node', label: t('proxies.fromCustomNode') }, { value: 'import', label: t('proxies.import') }, { value: 'manual', label: t('proxies.manual') }]} onChange={value => {
        const nextMode = value as Mode
        if (nextMode === 'manual') patch({ content: defaultManualContent() })
        setMode(nextMode)
      }} className="w-full" /></label>
      {mode === 'custom-node' ? <div className="space-y-2"><label className="block space-y-1 text-sm">{t('proxies.customNode')}<Select disabled={saving} value={input.customNodeId ?? undefined} placeholder={t('proxies.selectCustomNode')} options={nodes.map(node => ({ value: node.id, label: `${node.name} · ${node.proxyType}` }))} onChange={value => patch({ customNodeId: String(value) })} className="w-full" /></label><p className="text-xs text-[var(--muted)]">{t(nodes.length ? 'proxies.customNodeHelp' : 'proxies.customNodesEmpty')}</p></div> : <>
        {mode === 'manual' ? <ManualProxyFields disabled={saving} onChange={content => patch({ content })} /> : <label className="block space-y-1 text-sm">{t('proxies.configuration')}<TextArea disabled={saving} placeholder={t('proxies.configurationPlaceholder')} rows={9} value={input.content ?? ''} onChange={event => patch({ content: event.target.value })} className="font-mono text-xs" /></label>}
        <p className="text-xs text-[var(--muted)]">{t('proxies.configurationHelp')}</p>
      </>}
      <Checkbox disabled={saving} checked={input.enabled} onChange={event => patch({ enabled: event.target.checked })}>{t('proxies.enabled')}</Checkbox>
    </div>
  </Modal>
}
