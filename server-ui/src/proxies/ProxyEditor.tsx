import { Button, Checkbox, Input, Modal, Select, TextArea } from '@acme/components'
import { useEffect, useRef, useState } from 'react'
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

export function ProxyEditor({ id, initial, nodes, onSaved, onClose }: {
  id: string | null; initial: ProxyInput; nodes: CustomNode[]; onSaved: () => Promise<void>; onClose: () => void
}) {
  const { t } = useI18n()
  const initialMode = initial.customNodeId || !id ? 'custom-node' : 'import'
  const [input, setInput] = useState(initial)
  const [mode, setMode] = useState<Mode>(initialMode)
  const [savedInput, setSavedInput] = useState(() => payload(initial, initialMode))
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const latest = useRef({ input: initial, mode: initialMode as Mode })
  const saved = useRef(savedInput)
  const pending = useRef(savedInput)
  const running = useRef<Promise<boolean> | null>(null)
  const alive = useRef(true)
  useEffect(() => { alive.current = true; return () => { alive.current = false } }, [])

  const flush = (): Promise<boolean> => {
    if (running.current) return running.current
    if (same(pending.current, saved.current)) { setError(''); return Promise.resolve(true) }
    setSaving(true); setError('')
    const operation = Promise.resolve().then(async () => {
      try {
        while (!same(pending.current, saved.current)) {
          const sent = pending.current
          if (!sent.name || (!sent.customNodeId && !sent.content?.trim())) {
            throw new Error(t(!sent.name ? 'proxies.namePlaceholder' : 'proxies.selectCustomNode'))
          }
          const configUnchanged = sent.engine === saved.current.engine && sent.content === saved.current.content && sent.customNodeId === saved.current.customNodeId
          const request = configUnchanged ? { name: sent.name, engine: sent.engine, enabled: sent.enabled } : sent
          await proxyApi.save(id, request)
          saved.current = sent
          if (alive.current) setSavedInput(sent)
          void onSaved()
        }
        return true
      } catch (reason) {
        if (alive.current) setError(reason instanceof Error ? reason.message : String(reason))
        return false
      } finally {
        running.current = null
        if (alive.current) setSaving(false)
      }
    })
    running.current = operation
    return operation
  }
  const commit = () => {
    if (!id) return Promise.resolve(true)
    pending.current = payload(latest.current.input, latest.current.mode)
    return flush()
  }
  const patch = (patch: Partial<ProxyInput>, immediate = false) => {
    const next = { ...latest.current.input, ...patch }
    latest.current = { ...latest.current, input: next }
    setInput(next)
    if (immediate) void commit()
  }
  const close = async () => { if (!id || await commit()) onClose() }
  const create = async () => {
    const next = payload(latest.current.input, latest.current.mode)
    if (!next.name || (mode === 'custom-node' && !next.customNodeId)) { setError(t(!next.name ? 'proxies.namePlaceholder' : 'proxies.selectCustomNode')); return }
    setSaving(true); setError('')
    try { await proxyApi.save(null, next); await onSaved(); onClose() }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { if (alive.current) setSaving(false) }
  }
  const disabled = !id && saving
  const dirty = !same(payload(input, mode), savedInput)
  return <Modal open title={t(id ? 'proxies.edit' : 'proxies.add')} onCancel={() => { if (!disabled) void close() }} footer={<div className="flex items-center justify-between gap-3">
    <span role="status" className="text-xs text-[var(--muted)]">{id ? saving ? t('relay.saving') : dirty ? t('relay.unsaved') : t('common.saved') : ''}</span>
    <div className="flex gap-2"><Button disabled={disabled} onClick={() => void close()}>{t('common.close')}</Button>{!id ? <Button variant="primary" loading={saving} onClick={() => void create()}>{t('proxies.add')}</Button> : null}</div>
  </div>}>
    <div className="space-y-4">
      {id ? <p className="text-xs text-[var(--muted)]">{t('proxies.autoSaveHint')}</p> : null}
      {error ? <div role="alert" className="space-y-2 text-sm text-red-600"><p>{error}</p>{id ? <Button size="small" disabled={saving} onClick={() => void commit()}>{t('common.retry')}</Button> : null}</div> : null}
      <label className="block space-y-1 text-sm">{t('proxies.name')}<Input disabled={disabled} value={input.name} onChange={event => patch({ name: event.target.value })} onBlur={() => void commit()} placeholder={t('proxies.namePlaceholder')} /></label>
      <label className="block space-y-1 text-sm">{t('proxies.engine')}<Select disabled={disabled} value={input.engine ?? 'auto'} options={[{ value: 'auto', label: t('proxies.engineAuto') }, { value: 'sing-box', label: 'sing-box' }, { value: 'xray', label: 'Xray' }]} onChange={value => patch({ engine: value === 'auto' ? null : value as ProxyInput['engine'] }, true)} className="w-full" /></label>
      <label className="block space-y-1 text-sm">{t('proxies.inputMode')}<Select disabled={disabled} value={mode} options={[{ value: 'custom-node', label: t('proxies.fromCustomNode') }, { value: 'import', label: t('proxies.import') }, { value: 'manual', label: t('proxies.manual') }]} onChange={value => {
        const nextMode = value as Mode
        const nextInput = nextMode === 'manual' ? { ...latest.current.input, content: defaultManualContent() } : latest.current.input
        latest.current = { input: nextInput, mode: nextMode }; setMode(nextMode); setInput(nextInput)
        void commit()
      }} className="w-full" /></label>
      {mode === 'custom-node' ? <div className="space-y-2"><label className="block space-y-1 text-sm">{t('proxies.customNode')}<Select disabled={disabled} value={input.customNodeId ?? undefined} placeholder={t('proxies.selectCustomNode')} options={nodes.map(node => ({ value: node.id, label: `${node.name} · ${node.proxyType}` }))} onChange={value => patch({ customNodeId: String(value) }, true)} className="w-full" /></label><p className="text-xs text-[var(--muted)]">{t(nodes.length ? 'proxies.customNodeHelp' : 'proxies.customNodesEmpty')}</p></div> : <>
        {mode === 'manual' ? <ManualProxyFields disabled={disabled} onChange={content => patch({ content })} onCommit={content => patch({ content }, true)} /> : <label className="block space-y-1 text-sm">{t('proxies.configuration')}<TextArea disabled={disabled} placeholder={t('proxies.configurationPlaceholder')} rows={9} value={input.content ?? ''} onChange={event => patch({ content: event.target.value })} onBlur={() => void commit()} className="font-mono text-xs" /></label>}
        <p className="text-xs text-[var(--muted)]">{t('proxies.configurationHelp')}</p>
      </>}
      <Checkbox disabled={disabled} checked={input.enabled} onChange={event => patch({ enabled: event.target.checked }, true)}>{t('proxies.enabled')}</Checkbox>
    </div>
  </Modal>
}
