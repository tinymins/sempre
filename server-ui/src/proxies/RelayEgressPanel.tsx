import { Button, Card, Input, Select, Spin, useToast } from '@acme/components'
import { useEffect, useState } from 'react'
import { useI18n } from '../i18n/provider'
import type { MessageKey } from '../i18n/zh-CN'
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
  const [url, setUrl] = useState('https://raw.githubusercontent.com/dler-io/Rules/refs/heads/main/Clash/Provider/AdBlock.yaml')
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
      ...proxies.map((proxy) => ({ value: `proxy:${proxy.id}`, label: `${proxy.name} (${t(`proxies.status.${proxy.status}` as MessageKey)})`, disabled: !proxy.enabled })),
    ]
    if (outbound.type === 'proxy' && !proxies.some((proxy) => proxy.id === outbound.proxyId)) options.push({ value: outboundValue(outbound), label: t('proxies.unavailable'), disabled: true })
    return <Select aria-label={label} className="w-full min-w-0" value={outboundValue(outbound)} options={options} disabled={busy || loading} onChange={(value) => onChange(parseOutbound(value))} />
  }
  return <div className="min-w-0 space-y-5 pt-4">
    <p className="text-sm text-[var(--muted)]">{t('relay.description')}</p>
    {loading ? <Spin /> : null}
    {error ? <p role="alert" className="break-words text-sm text-red-600">{error}</p> : null}
    {stale ? <p role="alert" className="text-sm">{t('relay.stale')}</p> : null}
    {stale || (!form && !loading) ? <Button disabled={busy || loading} onClick={() => void load()}>{t('relay.reload')}</Button> : null}
    {form ? <>
      <Card size="small" className="min-w-0">
        <div className="space-y-4 p-1 sm:p-2">
          <div><h2 className="font-semibold">{t('relay.default')}</h2><p className="mt-2 text-sm text-[var(--muted)]">{t('relay.fallbackHint')}</p></div>
          <div className="max-w-xl">{select(form.defaultOutbound, (defaultOutbound) => setForm({ ...form, defaultOutbound }), t('relay.default'))}</div>
        </div>
      </Card>
      <Card size="small" className="min-w-0">
        <div className="space-y-4 p-1 sm:p-2">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0"><h2 className="font-semibold">{t('relay.hostRules')}</h2><p className="mt-2 text-sm text-[var(--muted)]">{t('relay.hostHint')}</p></div>
            <Button className="shrink-0" disabled={busy || loading} onClick={() => setForm({ ...form, hostRules: [...form.hostRules, { host: '', outbound: { type: 'system' } }] })}>{t('relay.addHost')}</Button>
          </div>
          {form.hostRules.length === 0 ? <p className="rounded-lg border border-dashed border-[var(--border)] p-4 text-sm text-[var(--muted)]">{t('relay.empty')}</p> : null}
          <div className="space-y-3">{form.hostRules.map((rule, index) => <div key={index} className="grid min-w-0 gap-3 rounded-lg border border-[var(--border)] p-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] md:items-end">
            <label className="flex min-w-0 flex-col gap-2 text-sm"><span>{t('relay.host')}</span><Input placeholder={t('relay.hostPlaceholder')} value={rule.host} disabled={busy || loading} onChange={(event) => setForm({ ...form, hostRules: form.hostRules.map((item, i) => i === index ? { ...item, host: event.target.value } : item) })} /></label>
            <div className="flex min-w-0 flex-col gap-2 text-sm"><span>{t('relay.outbound')}</span>{select(rule.outbound, (outbound) => setForm({ ...form, hostRules: form.hostRules.map((item, i) => i === index ? { ...item, outbound } : item) }), t('relay.outbound'))}</div>
            <Button className="justify-self-end" danger disabled={busy || loading} onClick={() => setForm({ ...form, hostRules: form.hostRules.filter((_, i) => i !== index) })}>{t('common.delete')}</Button>
          </div>)}</div>
        </div>
      </Card>
      <div className="flex flex-wrap items-center justify-end gap-3">{dirty ? <span className="text-sm text-[var(--muted)]">{t('relay.unsaved')}</span> : null}<Button variant="primary" disabled={!dirty || busy || loading || stale} loading={busy} onClick={() => void save()}>{t('common.save')}</Button></div>
      <Card size="small" className="min-w-0">
        <div className="space-y-4 p-1 sm:p-2">
          <div><h2 className="font-semibold">{t('relay.test')}</h2><p className="mt-2 text-sm text-[var(--muted)]">{t('relay.testHint')}</p></div>
          <div className="flex flex-col gap-3 sm:flex-row sm:items-end">
            <label className="flex min-w-0 flex-1 flex-col gap-2 text-sm"><span>{t('relay.url')}</span><Input placeholder={t('relay.urlPlaceholder')} value={url} disabled={busy || loading} onChange={(event) => { setUrl(event.target.value); setResult(null) }} /></label>
            <Button className="shrink-0 self-end" disabled={dirty || stale || busy || loading || !url.trim()} loading={busy} onClick={() => void test()}>{t('relay.testAction')}</Button>
          </div>
          {result ? <dl role="status" className="grid grid-cols-[6rem_minmax(0,1fr)] gap-x-4 gap-y-3 border-t border-[var(--border)] pt-4 text-sm">
            <dt className="text-[var(--muted)]">{t('relay.host')}</dt><dd className="min-w-0 break-all">{result.host}</dd>
            <dt className="text-[var(--muted)]">{t('relay.matched')}</dt><dd className="min-w-0 break-all">{result.matchedHost ?? t('relay.default')}</dd>
            <dt className="text-[var(--muted)]">{t('relay.outbound')}</dt><dd className="min-w-0 break-all">{result.outbound.type === 'system' ? t('proxies.system') : result.outbound.type === 'direct' ? t('relay.direct') : result.outboundName}</dd>
            <dt className="text-[var(--muted)]">HTTP</dt><dd>{result.httpStatus ?? '—'}</dd>
            <dt className="text-[var(--muted)]">{t('relay.elapsed')}</dt><dd>{number(result.elapsedMs)} ms</dd>
            <dt className="text-[var(--muted)]">{t('relay.bytes')}</dt><dd>{number(result.bodyBytes)} B</dd>
            <dt className="text-[var(--muted)]">{t('common.status')}</dt><dd>{result.ok ? t('relay.success') : t('relay.failure')}</dd>
            {result.error ? <><dt className="text-[var(--muted)]">{t('relay.error')}</dt><dd className="min-w-0 break-all text-red-600">{result.error}</dd></> : null}
          </dl> : null}
        </div>
      </Card>
    </> : null}
  </div>
}
