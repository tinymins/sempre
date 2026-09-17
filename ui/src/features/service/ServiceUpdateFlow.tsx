import { createContext, useContext, useState, type ReactNode } from 'react'
import { loadSession } from '../../lib/api'
import { useSession } from '../../lib/session'
import { clearServiceUpdateMarker, readServiceUpdateMarker, useServiceUpdateTask } from '../../lib/useServiceUpdateTask'
import { ServiceUpdateModal } from './ServiceUpdateModal'
import { ConfirmDialog } from '../../components/ui'
import { useI18n } from '../../lib/i18n'

const Context = createContext<(ReturnType<typeof useServiceUpdateTask> & { openProgress: () => void }) | null>(null)

export function ServiceUpdateFlow({ children }: { children: ReactNode }) {
  const update = useServiceUpdateTask()
  const { locale } = useI18n()
  const { session, setSession } = useSession()
  const [open, setOpen] = useState(false)
  const marker = readServiceUpdateMarker()
  const { query, mutation, uploadMutation, confirmMutation } = update
  const submitting = mutation.isPending || uploadMutation.isPending
  const task = submitting ? null : update.task
  const succeeded = task?.state === 'succeeded'
  const holding = Boolean(marker && task)

  function close() {
    setOpen(false)
    if (task?.state === 'failed' || mutation.isError || uploadMutation.isError) {
      clearServiceUpdateMarker()
      if (!loadSession()) setSession(null)
    }
  }

  function relogin() {
    clearServiceUpdateMarker()
    setSession(null)
    window.location.reload()
  }

  const awaitingConfirmation = task?.state === 'running' && task.stage === 'awaiting_confirmation'
  const zh = locale === 'zh-CN'
  const confirmDetail = task ? <div className="space-y-3"><p>{zh ? '安装包已下载并校验完成。目标版本读取自安装包，请确认是否继续安装。' : 'The package has been downloaded and verified. Its target version was read from the package; confirm to continue.'}</p><div className="flex items-center justify-between rounded-md bg-[var(--surface-hover)] px-4 py-3 font-mono text-sm"><span>v{task.current_version.replace(/^v/, '')}</span><span>→</span><span>v{task.target_version.replace(/^v/, '')}</span></div></div> : null

  async function decide(confirmed: boolean) {
    if (!task) return
    await confirmMutation.mutateAsync({ id: task.id, confirmed })
    if (!confirmed) setOpen(false)
  }

  return <Context.Provider value={{ ...update, openProgress: () => setOpen(true) }}>
    {children}
    <ServiceUpdateModal open={open || Boolean(holding && (succeeded || task?.state === 'failed'))} task={task} targetVersion={marker?.targetVersion || ''}
      submitting={submitting} uploading={uploadMutation.isPending} disconnected={Boolean(task?.state === 'running' && task.stage === 'installing' && (query.isError || !session))}
      error={mutation.error?.message || uploadMutation.error?.message || confirmMutation.error?.message || (task?.state === 'failed' ? task.error : undefined)}
      onClose={close} onRelogin={relogin} />
    <ConfirmDialog open={Boolean(awaitingConfirmation)} title={zh ? '确认更新 Sempre？' : 'Confirm Sempre update?'} detail={confirmDetail} confirmLabel={zh ? '确认安装' : 'Install update'} cancelLabel={zh ? '取消更新' : 'Cancel update'} pending={confirmMutation.isPending} onCancel={() => void decide(false)} onConfirm={() => void decide(true)} />
  </Context.Provider>
}

export function useServiceUpdateFlow() {
  const value = useContext(Context)
  if (!value) throw new Error('ServiceUpdateFlow is missing')
  return value
}
