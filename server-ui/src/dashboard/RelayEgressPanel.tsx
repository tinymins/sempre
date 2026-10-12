import { Button, Card, Input, Select, Spin, useToast } from '@acme/components'
import { useEffect, useState } from 'react'
import { useI18n } from '../i18n/provider'
import { ServerApiError } from '../server-api'
import { relayEgressApi, type RelayEgressPolicy, type RelayEgressTest, type RelayOutbound } from './relay-egress-api'

const outboundValue = (outbound: RelayOutbound) => outbound.type === 'proxy' ? `proxy:${outbound.proxyId}` : outbound.type
const parseOutbound = (value: string): RelayOutbound => value.startsWith('proxy:') ? { type: 'proxy', proxyId: value.slice(6) } : { type: value as 'system' | 'direct' }
const policyValue = (policy: RelayEgressPolicy) => JSON.stringify([policy.defaultOutbound, policy.hostRules])
const message = (reason: unknown) => reason instanceof Error ? reason.message : String(reason)

export function RelayEgressPanel() {
  const { t, number } = useI18n()
  const toast = useToast()
  const [saved, setSaved] = useState<RelayEgressPolicy | null>(null)
  const [form, setForm] = useState<RelayEgressPolicy | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [stale, setStale] = useState(false)
  const [url, setUrl] = useState('')
  const [result, setResult] = useState<RelayEgressTest | null>(null)
  const apply = (next: RelayEgressPolicy) => { setSaved(next); setForm(next); setStale(false); setResult(null) }
  const load = async () => {
    setLoading(true); setError('')
    try { apply(await relayEgressApi.get()) }
    catch (reason) { setError(message(reason)) }
    finally { setLoading(false) }
  }
  useEffect(() => {
    let active = true
    void relayEgressApi.get().then((next) => { if (active) apply(next) })
      .catch((reason) => { if (active) setError(message(reason)) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [])
  const dirty = !!form && !!saved && policyValue(form) !== policyValue(saved)
  const save = async () => {
    if (!form) return
    setBusy(true); setError('')
    try { apply(await relayEgressApi.save(form)); toast.success(t('common.saved')) }
    catch (reason) { setError(message(reason)); if (reason instanceof ServerApiError && reason.status === 409) setStale(true) }
    finally { setBusy(false) }
  }
  const test = async () => {
    setBusy(true); setError(''); setResult(null)
    try { setResult(await relayEgressApi.test(url)) }
    catch (reason) { setError(message(reason)) }
    finally { setBusy(false) }
  }
  const select = (outbound: RelayOutbound, onChange: (outbound: RelayOutbound) => void, label: string) => {
    const proxies = form?.availableProxies ?? []
    const options = [
      { value: 'system', label: t('proxies.system') },
      { value: 'direct', label: t('relay.direct') },
      ...proxies.map((proxy) => ({ value: `proxy:${proxy.id}`, label: `${proxy.name} (${proxy.status})`, disabled: !proxy.enabled })),
    ]
    if (outbound.type === 'proxy' && !proxies.some((proxy) => proxy.id === outbound.proxyId)) options.push({ value: outboundValue(outbound), label: t('proxies.unavailable'), disabled: true })
    return <Select aria-label={label} className="w-full min-w-40" value={outboundValue(outbound)} options={options} disabled={busy || loading} onChange={(value) => onChange(parseOutbound(value))} />
  }
  return <Card className="space-y-4">
    <div><h2 className="font-semibold">{t('relay.title')}</h2><p className="mt-1 text-sm text-[var(--muted)]">{t('relay.description')}</p></div>
    {loading ? <Spin /> : null}
    {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
    {stale ? <p role="alert" className="text-sm">{t('relay.stale')}</p> : null}
    {stale || (!form && !loading) ? <Button disabled={busy || loading} onClick={() => void load()}>{t('relay.reload')}</Button> : null}
    {form ? <>
      <label className="block space-y-1 text-sm">{t('relay.default')}{select(form.defaultOutbound, (defaultOutbound) => setForm({ ...form, defaultOutbound }), t('relay.default'))}</label>
      <p className="text-xs text-[var(--muted)]">{t('relay.fallbackHint')}</p>
      <div className="space-y-2"><h3 className="text-sm font-semibold">{t('relay.hostRules')}</h3><p className="text-xs text-[var(--muted)]">{t('relay.hostHint')}</p>
        {form.hostRules.length === 0 ? <p className="text-sm text-[var(--muted)]">{t('relay.empty')}</p> : null}
        {form.hostRules.map((rule, index) => <div key={index} className="grid items-start gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
          <Input aria-label={t('relay.host')} placeholder="example.com" value={rule.host} disabled={busy || loading} onChange={(event) => setForm({ ...form, hostRules: form.hostRules.map((item, i) => i === index ? { ...item, host: event.target.value } : item) })} />
          {select(rule.outbound, (outbound) => setForm({ ...form, hostRules: form.hostRules.map((item, i) => i === index ? { ...item, outbound } : item) }), t('relay.outbound'))}
          <Button disabled={busy || loading} onClick={() => setForm({ ...form, hostRules: form.hostRules.filter((_, i) => i !== index) })}>{t('common.delete')}</Button>
        </div>)}
        <Button disabled={busy || loading} onClick={() => setForm({ ...form, hostRules: [...form.hostRules, { host: '', outbound: { type: 'system' } }] })}>{t('relay.addHost')}</Button>
      </div>
      <div className="flex items-center gap-3"><Button variant="primary" disabled={!dirty || busy || loading || stale} loading={busy} onClick={() => void save()}>{t('common.save')}</Button>{dirty ? <span className="text-sm text-[var(--muted)]">{t('relay.unsaved')}</span> : null}</div>
      <div className="space-y-2 border-t border-[var(--border)] pt-4"><h3 className="text-sm font-semibold">{t('relay.test')}</h3><p className="text-xs text-[var(--muted)]">{t('relay.testHint')}</p><div className="flex flex-wrap gap-2"><Input aria-label={t('relay.url')} placeholder="https://example.com/resource" className="min-w-0 flex-1" value={url} disabled={busy || loading} onChange={(event) => { setUrl(event.target.value); setResult(null) }} /><Button disabled={dirty || stale || busy || loading || !url.trim()} onClick={() => void test()}>{t('proxies.test')}</Button></div>
        {result ? <dl role="status" className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1 text-sm"><dt>{t('relay.host')}</dt><dd className="break-all">{result.host}</dd><dt>{t('relay.matched')}</dt><dd className="break-all">{result.matchedHost ?? t('relay.default')}</dd><dt>{t('relay.outbound')}</dt><dd className="break-all">{result.outbound.type === 'system' ? t('proxies.system') : result.outbound.type === 'direct' ? t('relay.direct') : result.outboundName}</dd><dt>HTTP</dt><dd>{result.httpStatus ?? '—'}</dd><dt>{t('relay.elapsed')}</dt><dd>{number(result.elapsedMs)} ms</dd><dt>{t('relay.bytes')}</dt><dd>{number(result.bodyBytes)} B</dd><dt>{t('common.status')}</dt><dd>{result.ok ? t('relay.success') : t('relay.failure')}</dd>{result.error ? <><dt>{t('relay.error')}</dt><dd className="break-all text-red-600">{result.error}</dd></> : null}</dl> : null}
      </div>
    </> : null}
  </Card>
}
