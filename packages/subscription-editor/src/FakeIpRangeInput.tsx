import { Alert, Button, Input } from '@acme/components'
import { useEffect, useState } from 'react'
import { useEditorI18n } from './i18n'

export interface FakeIpRouteCheck {
  conflicts: string[]
  recommendation: { range: string; private_network_fallback: boolean } | null
}

export type CheckFakeIpRange = (range: string, signal: AbortSignal) => Promise<FakeIpRouteCheck>

export function FakeIpRangeInput({ value, disabled, enabled, check, onChange, onApplyRecommendation, label }: {
  value: string; disabled: boolean; enabled: boolean; label: string
  check?: CheckFakeIpRange; onApplyRecommendation?: (range: string) => void; onChange: (value: string) => void
}) {
  const { t } = useEditorI18n()
  const [state, setState] = useState<{ range: string; result?: FakeIpRouteCheck; error?: boolean }>()
  useEffect(() => {
    if (!check || !enabled || !value?.trim()) return
    const controller = new AbortController()
    const timer = window.setTimeout(() => {
      void check(value, controller.signal).then(result => {
        if (!controller.signal.aborted) setState({ range: value, result })
      }).catch(() => {
        if (!controller.signal.aborted) setState({ range: value, error: true })
      })
    }, 350)
    return () => { window.clearTimeout(timer); controller.abort() }
  }, [check, enabled, value])
  const current = enabled && check && state?.range === value ? state : undefined
  const result = current?.result
  const recommendation = result?.recommendation
  return <div className="space-y-2">
    <Input aria-label={label} value={value} disabled={disabled} onChange={event => onChange(event.target.value)} />
    {result && result.conflicts.length > 0 ? <Alert type="warning" showIcon message={t('dns.routeConflict')}
      description={<div className="space-y-2 break-words">
        <p>{t('dns.conflictingRoutes', { routes: result.conflicts.join(', ') })}</p>
        {recommendation ? <>
          <p>{t('dns.recommendedRange', { range: recommendation.range })}</p>
          {recommendation.private_network_fallback ? <p>{t('dns.fallbackRange')}</p> : null}
          <Button size="small" disabled={disabled && !onApplyRecommendation} onClick={() => (onApplyRecommendation ?? onChange)(recommendation.range)}>{t('dns.useRecommendedRange')}</Button>
        </> : <p>{t('dns.noAvailableRange')}</p>}
      </div>} /> : null}
    {current?.error ? <Alert type="warning" showIcon message={t('dns.routeCheckFailed')} /> : null}
  </div>
}
