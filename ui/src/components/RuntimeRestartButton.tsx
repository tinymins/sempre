import { useState } from 'react'
import { useIsMutating, useQuery } from '@tanstack/react-query'
import { Button } from '@acme/components'
import { LoaderCircle, RotateCw, ScrollText } from 'lucide-react'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { useRestartTask } from '../lib/useRestartTask'
import type { ManagedRuntimeStatus } from '../lib/types'
import { RestartChangeSummary, type RuntimePendingChange } from './RestartChangeSummary'
import { RuntimeRestartModal } from './RuntimeRestartModal'
import { ConfirmDialog } from './ui'

type RuntimeStatusWithChanges = ManagedRuntimeStatus & { pending_changes: RuntimePendingChange[] }

export function RuntimeRestartButton({ showLabel = false, panel = false }: { showLabel?: boolean; panel?: boolean }) {
  const { locale, t } = useI18n()
  const { session } = useSession()
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [taskOpen, setTaskOpen] = useState(false)
  const [submittedAt, setSubmittedAt] = useState(() => new Date().toISOString())
  const { task, query, mutation } = useRestartTask()
  const submitting = useIsMutating({ mutationKey: ['runtime', 'restart-task'] }) > 0
  const runtimeStatus = useQuery({
    queryKey: ['runtime', 'status'],
    queryFn: () => api<RuntimeStatusWithChanges>(session!, '/runtime/status'),
    enabled: Boolean(session),
    refetchInterval: (query) => query.state.data?.pending || ['starting', 'stopping', 'restarting'].includes(query.state.data?.runtime_state || '') ? 1000 : 3000,
  })
  const restarting = submitting || task?.state === 'running'
  const restartDisabled = restarting || !runtimeStatus.data?.actions?.restart.allowed || ['starting', 'stopping', 'restarting'].includes(runtimeStatus.data?.runtime_state || '')
  const needsRestart = Boolean(runtimeStatus.data?.pending)
  const label = restarting ? (locale === 'zh-CN' ? '正在重启核心 · 查看日志' : 'Restarting core · view log') : t(panel ? 'restartCore' : 'restartNow')
  const visibleTask = mutation.error && task && task.started_at < submittedAt ? null : task

  return <>
    <span className="relative inline-flex items-center gap-1">
      <Button variant={showLabel || panel ? 'default' : 'text'} className={showLabel ? 'h-9' : '!size-9 !p-0'} title={label} aria-label={label} disabled={!restarting && restartDisabled} onClick={() => restarting ? setTaskOpen(true) : setConfirmOpen(true)}>
        {restarting ? <LoaderCircle size={18} className="animate-spin" /> : <RotateCw size={18} />}
        {showLabel ? label : null}
      </Button>
      {needsRestart && !restarting ? <span data-restart-required aria-hidden="true" className="pointer-events-none absolute left-6 top-1.5 size-2 rounded-full bg-red-500 ring-2 ring-[var(--background)]" /> : null}
      {task ? <Button variant="text" className="!size-8 !p-0" title={locale === 'zh-CN' ? '查看重启任务' : 'View restart task'} aria-label={locale === 'zh-CN' ? '查看重启任务' : 'View restart task'} onClick={() => { mutation.reset(); setTaskOpen(true) }}><ScrollText size={16} /></Button> : null}
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
    {taskOpen ? <RuntimeRestartModal open task={visibleTask} submittedAt={submittedAt} submitting={submitting} error={mutation.error?.message || query.error?.message} onClose={() => setTaskOpen(false)} /> : null}
  </>
}
