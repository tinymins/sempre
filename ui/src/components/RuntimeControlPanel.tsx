import { useState, type ReactNode } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { AlertCircle, CircleCheck, Clock3, Play, RotateCw, Square, Terminal } from 'lucide-react'
import { Link } from 'react-router-dom'
import { api } from '../lib/api'
import { compactHash, formatDate, formatDuration } from '../lib/format'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { ManagedRuntimeStatus } from '../lib/types'
import { Badge, Button, Card, ConfirmDialog, Spinner } from './ui'

type RuntimeAction = 'start' | 'stop' | 'restart'

const transientStates = new Set(['starting', 'stopping', 'restarting'])

export function RuntimeControlPanel() {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [confirmStop, setConfirmStop] = useState(false)
  const [notice, setNotice] = useState('')
  const status = useQuery({
    queryKey: ['runtime', 'status'],
    queryFn: () => api<ManagedRuntimeStatus>(session!, '/runtime/status'),
    refetchInterval: (query) => transientStates.has(query.state.data?.runtime_state || '') ? 500 : 3000,
  })
  const action = useMutation({
    mutationFn: (operation: RuntimeAction) => api<{ action: RuntimeAction; status: ManagedRuntimeStatus }>(session!, `/runtime/${operation}`, { method: 'POST' }),
    onSuccess: (result) => {
      setNotice(t('operationAccepted'))
      setConfirmStop(false)
      queryClient.setQueryData(['runtime', 'status'], result.status)
      void queryClient.invalidateQueries({ queryKey: ['runtime', 'status'] })
      void queryClient.invalidateQueries({ queryKey: ['system'] })
    },
    onError: (error) => setNotice(error.message),
  })
  const value = status.data
  const runtimeState = value?.runtime_state || 'idle'
  const desiredState = value?.desired_state || 'running'
  const tone = runtimeTone(runtimeState)
  const actionPending = action.isPending ? action.variables : null

  function run(operation: RuntimeAction) {
    setNotice('')
    action.mutate(operation)
  }

  return <>
    <Card className="min-w-0 overflow-hidden">
      <div className="flex flex-wrap items-start justify-between gap-4 border-b border-[var(--border)] p-4 md:p-5">
        <div>
          <div className="flex items-center gap-2"><Terminal size={18} className="text-emerald-600" /><h2 className="text-sm font-semibold">{t('managedRuntime')}</h2></div>
          <div className="mt-3 flex flex-wrap items-center gap-x-5 gap-y-2 text-sm">
            <span className="inline-flex items-center gap-2"><CircleCheck size={15} className="text-emerald-600" />{t('sempreService')}<Badge tone="success">{t('online')}</Badge></span>
            <span className="inline-flex items-center gap-2"><span className={`size-2 rounded-full ${runtimeDot(runtimeState)}`} />{t('managedCore')}<Badge tone={tone}>{runtimeLabel(runtimeState, t)}</Badge></span>
          </div>
        </div>
        <div className="flex h-9 items-center gap-1">
          <RuntimeButton label={t('startCore')} reason={value?.actions.start.reason} disabled={!value?.actions.start.allowed || action.isPending} pending={actionPending === 'start'} onClick={() => run('start')}><Play size={17} /></RuntimeButton>
          <RuntimeButton label={t('stopCore')} reason={value?.actions.stop.reason} disabled={!value?.actions.stop.allowed || action.isPending} pending={actionPending === 'stop'} danger onClick={() => setConfirmStop(true)}><Square size={16} /></RuntimeButton>
          <RuntimeButton label={t('restartCore')} reason={value?.actions.restart.reason} disabled={!value?.actions.restart.allowed || action.isPending} pending={actionPending === 'restart'} onClick={() => run('restart')}><RotateCw size={17} /></RuntimeButton>
        </div>
      </div>
      {notice ? <div className={`border-b border-[var(--border)] px-4 py-2 text-sm md:px-5 ${action.isError ? 'bg-red-500/8 text-red-700 dark:text-red-300' : 'bg-emerald-500/8 text-emerald-700 dark:text-emerald-300'}`}>{notice}</div> : null}
      <div className="grid gap-x-5 gap-y-4 p-4 sm:grid-cols-2 md:p-5 lg:grid-cols-4">
        <RuntimeInfo label={t('desiredState')} value={desiredState === 'running' ? t('running') : t('stopped')} />
        <RuntimeInfo label={t('actualState')} value={runtimeLabel(runtimeState, t)} />
        <RuntimeInfo label={t('core')} value={value?.active?.exact_reference || '-'} mono />
        <RuntimeInfo label={t('source')} value={value?.active ? value.active.repository || t('official') : '-'} mono />
        <RuntimeInfo label={t('selectedReference')} value={value?.active?.ref || '-'} mono />
        <RuntimeInfo label={t('version')} value={value?.active?.version || '-'} mono />
        <RuntimeInfo label={t('configuration')} value={compactHash(value?.active?.config_hash)} mono />
        <RuntimeInfo label="PID" value={value?.pid ? String(value.pid) : '-'} />
        <RuntimeInfo label={t('runtimeUptime')} value={formatDuration(value?.uptime_seconds)} />
        <RuntimeInfo label={t('restarts')} value={String(value?.restart_count || 0)} />
        <RuntimeInfo label={t('lastTransition')} value={formatDate(value?.last_transition || undefined)} />
      </div>
      {value?.pending ? <div className="mx-4 mb-4 flex items-start gap-2 rounded-md border border-amber-500/35 bg-amber-500/8 px-3 py-2 text-sm text-amber-800 dark:text-amber-300 md:mx-5 md:mb-5"><Clock3 size={16} className="mt-0.5 shrink-0" /><span>{t('pendingChange')}</span></div> : null}
      {value?.last_error ? <div className="mx-4 mb-4 flex flex-wrap items-start gap-3 rounded-md border border-red-500/35 bg-red-500/8 px-3 py-3 text-sm text-red-800 dark:text-red-300 md:mx-5 md:mb-5"><AlertCircle size={17} className="mt-0.5 shrink-0" /><div className="min-w-0 flex-1"><p className="font-medium">{t('lastError')}</p><p className="mt-1 break-words text-xs leading-5">{value.last_error}</p></div><Link className="inline-flex h-8 items-center rounded-md border border-[var(--border)] bg-[var(--surface)] px-2 text-xs font-medium text-[var(--text)] hover:bg-[var(--surface-hover)]" to="/logs">{t('viewLogs')}</Link></div> : value?.last_exit ? <div className="border-t border-[var(--border)] px-4 py-3 text-xs text-[var(--muted)] md:px-5"><span className="font-medium">{t('lastExit')}:</span> {value.last_exit}</div> : null}
    </Card>
    <ConfirmDialog open={confirmStop} title={t('coreStopTitle')} detail={t('coreStopWarning')} confirmLabel={t('stopCore')} cancelLabel={t('cancel')} pending={actionPending === 'stop'} onCancel={() => setConfirmStop(false)} onConfirm={() => run('stop')} />
  </>
}

function RuntimeButton({ label, reason, disabled, pending, danger = false, onClick, children }: { label: string; reason?: string; disabled: boolean; pending: boolean; danger?: boolean; onClick: () => void; children: ReactNode }) {
  return <span className="inline-flex" title={disabled && reason ? reason : label}><Button size="icon" variant={danger ? 'danger' : 'secondary'} aria-label={label} disabled={disabled} onClick={onClick}>{pending ? <Spinner /> : children}</Button></span>
}

function RuntimeInfo({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return <div className="min-w-0"><p className="text-xs text-[var(--muted)]">{label}</p><p className={`mt-1 break-words text-sm font-medium ${mono ? 'font-mono text-xs' : ''}`} title={value}>{value}</p></div>
}

function runtimeTone(state: string): 'neutral' | 'success' | 'warning' | 'danger' | 'info' {
  if (state === 'running') return 'success'
  if (state === 'failed') return 'danger'
  if (transientStates.has(state)) return 'info'
  if (state === 'idle') return 'warning'
  return 'neutral'
}

function runtimeDot(state: string) {
  if (state === 'running') return 'bg-emerald-500'
  if (state === 'failed') return 'bg-red-500'
  if (transientStates.has(state)) return 'bg-cyan-500 animate-pulse'
  if (state === 'idle') return 'bg-amber-500'
  return 'bg-zinc-400'
}

function runtimeLabel(state: string, t: ReturnType<typeof useI18n>['t']) {
  switch (state) {
    case 'running': return t('running')
    case 'stopped': return t('stopped')
    case 'starting': return t('starting')
    case 'stopping': return t('stopping')
    case 'restarting': return t('restarting')
    case 'failed': return t('failed')
    default: return t('idle')
  }
}
