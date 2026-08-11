import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Modal } from '@acme/components'
import { CheckCircle2, CircleAlert, Info, WandSparkles } from 'lucide-react'
import { Badge, Button, Card, Spinner } from '../../components/ui'
import { api } from '../../lib/api'
import { useI18n } from '../../lib/i18n'
import { useSession } from '../../lib/session'
import type { AutoConfigApplyResult, AutoConfigCandidate, AutoConfigCheck, AutoConfigReport } from './types'

export function AutoConfigureCard() {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [open, setOpen] = useState(false)
  const [notice, setNotice] = useState('')
  const diagnose = useMutation({
    mutationFn: () => api<AutoConfigReport>(session!, '/cores/auto/diagnose', { method: 'POST' }),
    onSuccess: () => { setNotice(''); setOpen(true) },
    onError: (error) => setNotice(error.message),
  })
  const apply = useMutation({
    mutationFn: (candidateID: string) => api<AutoConfigApplyResult>(session!, '/cores/auto/apply', {
      method: 'POST', body: JSON.stringify({ candidate_id: candidateID }),
    }),
    onSuccess: async (result) => {
      setOpen(false)
      setNotice(`${t('autoConfigApplied')}: ${result.recommendation.reference}`)
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['cores'] }),
        queryClient.invalidateQueries({ queryKey: ['system'] }),
        queryClient.invalidateQueries({ queryKey: ['runtime', 'status'] }),
        queryClient.invalidateQueries({ queryKey: ['subscriptions'] }),
      ])
    },
  })
  const report = diagnose.data
  const recommendation = report?.recommendation

  return <>
    <Card className="overflow-hidden">
      <div className="flex flex-wrap items-center justify-between gap-4 p-4 md:p-5">
        <div className="flex min-w-0 items-start gap-3">
          <span className="grid size-9 shrink-0 place-items-center rounded-md bg-emerald-500/12 text-emerald-600"><WandSparkles size={18} /></span>
          <div className="min-w-0"><h2 className="text-sm font-semibold">{t('autoConfigTitle')}</h2><p className="mt-1 max-w-3xl text-sm leading-6 text-[var(--muted)]">{t('autoConfigDetail')}</p></div>
        </div>
        <Button variant="primary" disabled={diagnose.isPending || apply.isPending} onClick={() => diagnose.mutate()}>{diagnose.isPending ? <Spinner /> : <WandSparkles size={16} />}{diagnose.isPending ? t('diagnosing') : t('autoConfigAction')}</Button>
      </div>
      {notice ? <div className={`border-t border-[var(--border)] px-4 py-2 text-sm md:px-5 ${diagnose.isError ? 'bg-red-500/8 text-red-700 dark:text-red-300' : 'bg-emerald-500/8 text-emerald-700 dark:text-emerald-300'}`}>{notice}</div> : null}
    </Card>
    <Modal
      open={open}
      title={t('autoConfigResult')}
      okText={t('applyRecommendation')}
      cancelText={t('cancel')}
      width={680}
      confirmLoading={apply.isPending}
      okButtonProps={{ disabled: !recommendation }}
      cancelButtonProps={{ disabled: apply.isPending }}
      maskClosable={!apply.isPending}
      keyboard={!apply.isPending}
      onCancel={() => setOpen(false)}
      onOk={() => recommendation ? apply.mutateAsync(recommendation.id) : undefined}
      destroyOnClose
      centered
    >
      {report ? <DiagnosisResult report={report} /> : null}
      {apply.error ? <div className="mt-4 rounded-md border border-red-500/35 bg-red-500/8 px-3 py-2 text-sm text-red-700 dark:text-red-300">{apply.error.message}</div> : null}
    </Modal>
  </>
}

function DiagnosisResult({ report }: { report: AutoConfigReport }) {
  const { t } = useI18n()
  const recommendation = report.recommendation
  return <div className="space-y-5">
    <p className="text-sm leading-6 text-[var(--muted)]">{t('autoConfigBoundary')}</p>
    <div>
      <h3 className="text-xs font-semibold uppercase tracking-wide text-[var(--muted)]">{t('systemChecks')}</h3>
      <div className="mt-2 grid gap-2 sm:grid-cols-2">{report.checks.map((check) => <CheckItem key={check.id} check={check} />)}</div>
    </div>
    {recommendation ? <CandidateCard candidate={recommendation} recommended /> : <div className="rounded-md border border-amber-500/35 bg-amber-500/8 p-3 text-sm text-amber-800 dark:text-amber-300">{t('noAutoRecommendation')}</div>}
    {report.candidates.length > 1 ? <div><h3 className="text-xs font-semibold uppercase tracking-wide text-[var(--muted)]">{t('alternativeChoices')}</h3><div className="mt-2 grid gap-2">{report.candidates.slice(1).map((candidate) => <CandidateCard key={candidate.id} candidate={candidate} />)}</div></div> : null}
  </div>
}

function CheckItem({ check }: { check: AutoConfigCheck }) {
  const { t } = useI18n()
  const Icon = check.status === 'pass' ? CheckCircle2 : check.status === 'warning' ? CircleAlert : Info
  const tone = check.status === 'pass' ? 'text-emerald-600' : check.status === 'warning' ? 'text-amber-600' : 'text-cyan-600'
  return <div className="flex items-start gap-2 rounded-md bg-[var(--surface-hover)] px-3 py-2"><Icon size={16} className={`mt-0.5 shrink-0 ${tone}`} /><div className="min-w-0"><p className="text-sm font-medium">{checkLabel(check.id, t)}</p>{check.detail ? <p className="mt-0.5 break-words text-xs text-[var(--muted)]">{check.detail}</p> : null}</div></div>
}

function CandidateCard({ candidate, recommended = false }: { candidate: AutoConfigCandidate; recommended?: boolean }) {
  const { t } = useI18n()
  return <div className={`rounded-md border p-3 ${recommended ? 'border-emerald-500/45 bg-emerald-500/7' : 'border-[var(--border)]'}`}>
    <div className="flex flex-wrap items-center gap-2"><span className="font-mono text-sm font-semibold">{candidate.reference}</span>{recommended ? <Badge tone="success">{t('recommended')}</Badge> : null}{candidate.installed ? <Badge>{t('installed')}</Badge> : null}{candidate.selected ? <Badge tone="info">{t('selected')}</Badge> : null}<span className="ml-auto text-xs text-[var(--muted)]">{t('score')} {candidate.score}</span></div>
    <p className="mt-2 text-sm">{modeLabel(candidate.configuration_mode, t)}</p>
    <ul className="mt-2 space-y-1 text-xs leading-5 text-[var(--muted)]">{candidate.reasons.map((reason) => <li key={reason}>• {reasonLabel(reason, t)}</li>)}</ul>
    {candidate.warnings?.length ? <div className="mt-2 rounded bg-amber-500/10 px-2 py-1.5 text-xs leading-5 text-amber-800 dark:text-amber-300">{candidate.warnings.map((warning) => <p key={warning}>{warningLabel(warning, t)}</p>)}</div> : null}
  </div>
}

type Translate = ReturnType<typeof useI18n>['t']

function checkLabel(value: string, t: Translate) {
  return ({ platform: t('checkPlatform'), 'system-dns-boundary': t('checkDNSBoundary'), 'active-profile': t('checkActiveProfile') } as Record<string, string>)[value] || value
}

function modeLabel(value: string, t: Translate) {
  return ({ 'macos-tun-real-ip': t('modeMacRealIP'), 'macos-tun-external-dns': t('modeMacExternalDNS'), 'platform-tun': t('modePlatformTUN'), 'mihomo-tun': t('modeMihomoTUN') } as Record<string, string>)[value] || value
}

function reasonLabel(value: string, t: Translate) {
  return ({ 'macos-standalone-compatible': t('reasonMacStandalone'), 'legacy-destination-override': t('reasonLegacyOverride'), 'stable-release': t('reasonStableRelease'), 'broad-protocol-support': t('reasonBroadProtocols') } as Record<string, string>)[value] || value
}

function warningLabel(value: string, t: Translate) {
  return ({ 'legacy-core-version': t('warningLegacyCore'), 'external-system-dns-required': t('warningExternalDNS'), 'not-verified-for-standalone-macos': t('warningMacUnverified') } as Record<string, string>)[value] || value
}
