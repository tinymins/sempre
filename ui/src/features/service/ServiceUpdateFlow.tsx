import { createContext, useContext, useState, type ReactNode } from 'react'
import { loadSession } from '../../lib/api'
import { useSession } from '../../lib/session'
import { clearServiceUpdateMarker, readServiceUpdateMarker, useServiceUpdateTask } from '../../lib/useServiceUpdateTask'
import { ServiceUpdateModal } from './ServiceUpdateModal'

const Context = createContext<(ReturnType<typeof useServiceUpdateTask> & { openProgress: () => void }) | null>(null)

export function ServiceUpdateFlow({ children }: { children: ReactNode }) {
  const update = useServiceUpdateTask()
  const { session, setSession } = useSession()
  const [open, setOpen] = useState(false)
  const marker = readServiceUpdateMarker()
  const { query, mutation } = update
  const task = mutation.isPending ? null : update.task
  const succeeded = task?.state === 'succeeded'
  const holding = Boolean(marker && task)

  function close() {
    setOpen(false)
    if (task?.state === 'failed' || mutation.isError) {
      clearServiceUpdateMarker()
      if (!loadSession()) setSession(null)
    }
  }

  function relogin() {
    clearServiceUpdateMarker()
    setSession(null)
    window.location.reload()
  }

  return <Context.Provider value={{ ...update, openProgress: () => setOpen(true) }}>
    {children}
    <ServiceUpdateModal open={open || Boolean(holding && (succeeded || task?.state === 'failed'))} task={task} targetVersion={marker?.targetVersion || ''}
      submitting={mutation.isPending} disconnected={Boolean(task?.state === 'running' && task.stage === 'installing' && (query.isError || !session))}
      error={mutation.error?.message || (task?.state === 'failed' ? task.error : undefined)}
      onClose={close} onRelogin={relogin} />
  </Context.Provider>
}

export function useServiceUpdateFlow() {
  const value = useContext(Context)
  if (!value) throw new Error('ServiceUpdateFlow is missing')
  return value
}
