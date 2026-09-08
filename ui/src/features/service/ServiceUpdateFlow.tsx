import { createContext, useContext, useState, type ReactNode } from 'react'
import { useSession } from '../../lib/session'
import { clearServiceUpdateMarker, readServiceUpdateMarker, useServiceUpdateTask } from '../../lib/useServiceUpdateTask'
import { ServiceUpdateModal } from './ServiceUpdateModal'

const Context = createContext<(ReturnType<typeof useServiceUpdateTask> & { openProgress: () => void }) | null>(null)

export function ServiceUpdateFlow({ children }: { children: ReactNode }) {
  const update = useServiceUpdateTask()
  const { session, setSession } = useSession()
  const [open, setOpen] = useState(false)
  const marker = readServiceUpdateMarker()
  const { task, query, mutation } = update
  const succeeded = task?.state === 'succeeded'
  const holding = Boolean(marker && task && task.state !== 'failed')
  const awaitingLogin = !session && holding

  function close() {
    setOpen(false)
    if (task?.state === 'failed' || mutation.isError) clearServiceUpdateMarker()
  }

  function relogin() {
    clearServiceUpdateMarker()
    setSession(null)
    window.location.reload()
  }

  return <Context.Provider value={{ ...update, openProgress: () => setOpen(true) }}>
    {awaitingLogin ? <div className="min-h-screen bg-[var(--background)]" /> : children}
    <ServiceUpdateModal open={open || awaitingLogin || Boolean(holding && succeeded)} task={task} targetVersion={marker?.targetVersion || ''}
      submitting={mutation.isPending} disconnected={Boolean(task?.state === 'running' && task.stage === 'installing' && (query.isError || !session))}
      error={mutation.error?.message || (task?.state === 'failed' ? task.error : undefined)} allowClose={!awaitingLogin}
      onClose={close} onRelogin={relogin} />
  </Context.Provider>
}

export function useServiceUpdateFlow() {
  const value = useContext(Context)
  if (!value) throw new Error('ServiceUpdateFlow is missing')
  return value
}
