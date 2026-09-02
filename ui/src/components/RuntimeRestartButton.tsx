import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { RotateCw, X } from 'lucide-react'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { useRuntimeActionFeedback, type RuntimeActionNotice } from '../lib/useRuntimeActionFeedback'
import type { ManagedRuntimeStatus } from '../lib/types'
import { RestartChangeSummary, type RuntimePendingChange } from './RestartChangeSummary'
import { Button, ConfirmDialog, Spinner } from './ui'

type RuntimeStatusWithChanges = ManagedRuntimeStatus & { pending_changes: RuntimePendingChange[] }

export function RuntimeRestartButton({ showLabel = false }: { showLabel?: boolean }) {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [notice, setNotice] = useState<RuntimeActionNotice | null>(null)
  const runtimeStatus = useQuery({
    queryKey: ['runtime', 'status'],
    queryFn: () => api<RuntimeStatusWithChanges>(session!, '/runtime/status'),
    enabled: Boolean(session),
    refetchInterval: (query) => query.state.data?.pending || ['starting', 'stopping', 'restarting'].includes(query.state.data?.runtime_state || '') ? 1000 : false,
  })
  const acceptRuntimeAction = useRuntimeActionFeedback(runtimeStatus.data, setNotice)
  const restart = useMutation({
    mutationFn: () => api<{ action: string; status: ManagedRuntimeStatus }>(session!, '/runtime/restart', { method: 'POST' }),
    onSuccess: async (result) => {
      setConfirmOpen(false)
      queryClient.setQueryData(['runtime', 'status'], result.status)
      acceptRuntimeAction(result.status)
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['system'] }),
        queryClient.invalidateQueries({ queryKey: ['runtime', 'status'] }),
        queryClient.invalidateQueries({ queryKey: ['runtime', 'proxies'] }),
      ])
    },
    onError: (error) => setNotice({ message: error.message, tone: 'error' }),
  })
  const needsRestart = Boolean(runtimeStatus.data?.pending)

  return (
    <>
      <span className="relative inline-flex">
        <Button size={showLabel ? 'normal' : 'icon'} variant={showLabel ? 'secondary' : 'ghost'} title={t('restartNow')} aria-label={t('restartNow')} disabled={restart.isPending} onClick={() => setConfirmOpen(true)}>
          {restart.isPending ? <Spinner /> : <RotateCw size={18} />}
          {showLabel ? t('restartNow') : null}
        </Button>
        {needsRestart ? <span data-restart-required aria-hidden="true" className="pointer-events-none absolute right-1.5 top-1.5 size-2 rounded-full bg-red-500 ring-2 ring-[var(--background)]" /> : null}
      </span>
      <ConfirmDialog
        open={confirmOpen}
        title={t('coreRestartConfirmTitle')}
        detail={<RestartChangeSummary detail={t('coreRestartConfirmDetail')} changes={runtimeStatus.data?.pending_changes ?? []} />}
        confirmLabel={t('restartNow')}
        cancelLabel={t('cancel')}
        pending={restart.isPending}
        onCancel={() => { if (!restart.isPending) setConfirmOpen(false) }}
        onConfirm={() => restart.mutate()}
      />
      {notice ? (
        <div role={notice.tone === 'error' ? 'alert' : 'status'} className={`fixed right-4 top-20 z-50 flex max-w-md items-start gap-3 whitespace-pre-line rounded-lg border bg-[var(--surface)] px-4 py-3 text-sm shadow-lg ${notice.tone === 'error' ? 'border-red-500/40 text-red-700 dark:text-red-300' : 'border-emerald-500/40 text-emerald-700 dark:text-emerald-300'}`}>
          <span className="min-w-0 flex-1 break-words">{notice.message}</span>
          <button type="button" className="shrink-0 text-[var(--muted)] hover:text-[var(--text)]" title={t('close')} aria-label={t('close')} onClick={() => setNotice(null)}><X size={15} /></button>
        </div>
      ) : null}
    </>
  )
}
