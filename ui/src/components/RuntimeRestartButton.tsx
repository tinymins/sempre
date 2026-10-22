import { useState } from 'react'
import { useIsMutating, useQuery } from '@tanstack/react-query'
import { Button } from '@acme/components'
import { LoaderCircle, RotateCw, ScrollText } from 'lucide-react'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { useStartupTask } from '../lib/useStartupTask'
import type { ManagedRuntimeStatus } from '../lib/types'
import { formatPendingChange, pendingChangeCount, RestartChangeSummary, type RuntimePendingChange } from './RestartChangeSummary'
import { RuntimeRestartModal } from './RuntimeRestartModal'
import { ConfirmDialog } from './ui'

type RuntimeStatusWithChanges = ManagedRuntimeStatus & { pending_changes: RuntimePendingChange[] }

export function RuntimeRestartButton({ showLabel = false, panel = false }: { showLabel?: boolean; panel?: boolean }) {
  const { locale, t } = useI18n()
  const { session } = useSession()
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [taskOpen, setTaskOpen] = useState(false)
  const [submittedAt, setSubmittedAt] = useState(() => new Date().toISOString())
  const { task, query, mutation } = useStartupTask()
  const submitting = useIsMutating({ mutationKey: ['runtime', 'startup-task'] }) > 0
  const runtimeStatus = useQuery({
    queryKey: ['runtime', 'status'],
    queryFn: () => api<RuntimeStatusWithChanges>(session!, '/runtime/status'),
    enabled: Boolean(session),
    refetchInterval: (query) => query.state.data?.pending || ['starting', 'stopping', 'restarting'].includes(query.state.data?.runtime_state || '') ? 1000 : 3000,
  })
  const runtimeState = runtimeStatus.data?.runtime_state
  const runtimeBusy = ['starting', 'stopping', 'restarting'].includes(runtimeState || '')
  const restarting = submitting || task?.state === 'running' || runtimeBusy
  const restartDisabled = restarting || !runtimeStatus.data?.actions?.restart.allowed
  const needsRestart = Boolean(runtimeStatus.data?.pending)
  const pendingCount = pendingChangeCount(runtimeStatus.data?.pending_changes ?? [])
  const showPendingCount = needsRestart && pendingCount > 0 && !restarting && !showLabel && !panel
  const action = submitting ? 'restart' : task?.state === 'running' ? task.action : runtimeState === 'starting' ? 'start' : runtimeState === 'stopping' ? 'stop' : 'restart'
  const operationLabel = locale === 'zh-CN' ? { start: '正在启动核心 · 查看日志', stop: '正在停止核心 · 查看日志', restart: '正在重启核心 · 查看日志' } : { start: 'Starting core · view log', stop: 'Stopping core · view log', restart: 'Restarting core · view log' }
  const label = restarting ? operationLabel[action] : t(panel ? 'restartCore' : 'restartNow')
  const pendingFakeIp = runtimeStatus.data?.pending_changes?.find(change => change.type === 'fake_ip')
  const title = pendingFakeIp ? formatPendingChange(pendingFakeIp, t, locale) : showPendingCount
    ? locale === 'zh-CN' ? `有 ${pendingCount} 项改动待应用，点击重启核心` : `${pendingCount} changes pending. Click to restart the core.`
    : label
  const visibleTask = (mutation.error && task && task.started_at < submittedAt) || (runtimeBusy && task?.state !== 'running' && (!task?.finished_at || task.finished_at < (runtimeStatus.data?.last_transition || ''))) ? null : task

  return <>
    <span className="relative inline-flex items-center gap-1">
      <Button variant={showLabel || panel ? 'default' : 'text'} className={showLabel ? 'h-9' : showPendingCount ? '!h-9 !w-auto !px-2.5 text-amber-700 dark:text-amber-300' : '!size-9 !p-0'} title={title} aria-label={label} disabled={!restarting && restartDisabled} onClick={() => restarting ? setTaskOpen(true) : setConfirmOpen(true)}>
        {showPendingCount ? <span className="whitespace-nowrap text-xs">{locale === 'zh-CN' ? '待应用' : 'Pending'} {pendingCount}</span> : null}
        {restarting ? <LoaderCircle size={18} className="animate-spin" /> : <RotateCw size={18} />}
        {showLabel ? label : null}
      </Button>
      {needsRestart && !restarting && !showPendingCount ? <span data-restart-required aria-hidden="true" className="pointer-events-none absolute left-6 top-1.5 size-2 rounded-full bg-red-500 ring-2 ring-[var(--background)]" /> : null}
      {task || runtimeBusy ? <Button variant="text" className="!size-8 !p-0" title={locale === 'zh-CN' ? '查看核心操作日志' : 'View core operation log'} aria-label={locale === 'zh-CN' ? '查看核心操作日志' : 'View core operation log'} onClick={() => { mutation.reset(); setTaskOpen(true) }}><ScrollText size={16} /></Button> : null}
    </span>
    <ConfirmDialog open={confirmOpen} title={t('coreRestartConfirmTitle')}
      detail={<RestartChangeSummary detail={t('coreRestartConfirmDetail')} changes={runtimeStatus.data?.pending_changes ?? []} />}
      confirmLabel={t('restartNow')} cancelLabel={t('cancel')} pending={restartDisabled} onCancel={() => setConfirmOpen(false)}
      onConfirm={() => {
        if (restartDisabled) return
        setConfirmOpen(false)
        setSubmittedAt(new Date().toISOString())
        setTaskOpen(true)
        mutation.mutate()
      }} />
    {taskOpen ? <RuntimeRestartModal open task={visibleTask} submittedAt={runtimeBusy && !submitting ? runtimeStatus.data?.last_transition || submittedAt : submittedAt} submitting={submitting} runtimeState={runtimeState} error={mutation.error?.message || query.error?.message} onClose={() => setTaskOpen(false)} /> : null}
  </>
}
