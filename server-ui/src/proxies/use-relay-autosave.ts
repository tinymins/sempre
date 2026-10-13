import { useCallback, useEffect, useRef, useState } from 'react'
import { ServerApiError } from '../server-api'
import { relayEgressApi, type RelayEgressPolicy } from './relay-egress-api'

export type RelayDraft = Omit<RelayEgressPolicy, 'hostRules'> & {
  hostRules: (RelayEgressPolicy['hostRules'][number] & { id: string })[]
}

const payload = (draft: RelayEgressPolicy): RelayEgressPolicy => ({
  ...draft,
  hostRules: draft.hostRules.filter(rule => rule.host.trim()).map(rule => ({ host: rule.host.trim().toLowerCase(), outbound: rule.outbound })),
})
const value = (draft: RelayEgressPolicy) => JSON.stringify([draft.defaultOutbound, payload(draft).hostRules])

export function useRelayAutosave() {
  const [form, setForm] = useState<RelayDraft | null>(null)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const [stale, setStale] = useState(false)
  const [savedValue, setSavedValue] = useState('')
  const saved = useRef<RelayEgressPolicy | null>(null)
  const pending = useRef<RelayDraft | null>(null)
  const running = useRef(false)
  const blocked = useRef(false)
  const alive = useRef(false)

  const load = useCallback(async () => {
    setLoading(true); setError('')
    try {
      const next = await relayEgressApi.get()
      if (!alive.current) return
      saved.current = next
      pending.current = { ...next, hostRules: next.hostRules.map(rule => ({ ...rule, id: crypto.randomUUID() })) }
      blocked.current = false
      setForm(pending.current); setSavedValue(value(next)); setStale(false)
    } catch (reason) {
      if (alive.current) setError(reason instanceof Error ? reason.message : String(reason))
    } finally { if (alive.current) setLoading(false) }
  }, [])
  useEffect(() => {
    alive.current = true
    const timer = window.setTimeout(() => void load(), 0)
    return () => { alive.current = false; window.clearTimeout(timer) }
  }, [load])

  const flush = async () => {
    if (running.current || blocked.current || !pending.current || !saved.current) return
    running.current = true
    if (alive.current) { setSaving(true); setError('') }
    try {
      while (pending.current && saved.current && value(pending.current) !== value(saved.current)) {
        const sent = pending.current
        const next = await relayEgressApi.save({ ...payload(sent), revision: saved.current.revision })
        saved.current = next
        let current: RelayDraft = pending.current
        if (value(current) === value(sent)) {
          let index = 0
          current = { ...current, hostRules: current.hostRules.map(rule => rule.host.trim() ? { ...rule, ...next.hostRules[index++] } : rule) }
        }
        pending.current = { ...current, revision: next.revision, availableProxies: next.availableProxies }
        if (alive.current) { setForm(pending.current); setSavedValue(value(next)) }
      }
    } catch (reason) {
      blocked.current = reason instanceof ServerApiError && reason.status === 409
      if (alive.current) {
        setError(reason instanceof Error ? reason.message : String(reason))
        setStale(blocked.current)
      }
    } finally {
      running.current = false
      if (alive.current) setSaving(false)
    }
  }
  const change = (update: (current: RelayDraft) => RelayDraft) => {
    if (!pending.current) return
    const previous = value(pending.current)
    pending.current = update(pending.current)
    setForm(pending.current)
    if (value(pending.current) !== previous) void flush()
  }
  return { form, loading, saving, error, stale, load, retry: flush, change, dirty: !!form && value(form) !== savedValue }
}
