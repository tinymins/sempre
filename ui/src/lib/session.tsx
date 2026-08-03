import { createContext, useContext, useMemo, useState, type ReactNode } from 'react'
import { loadSession, saveSession } from './api'
import type { Session } from './types'

const Context = createContext<{ session: Session | null; setSession: (session: Session | null) => void } | null>(null)

export function SessionProvider({ children }: { children: ReactNode }) {
  const [session, updateSession] = useState<Session | null>(() => loadSession())
  const value = useMemo(() => ({
    session,
    setSession(next: Session | null) {
      saveSession(next)
      updateSession(next)
    },
  }), [session])
  return <Context.Provider value={value}>{children}</Context.Provider>
}

export function useSession() {
  const value = useContext(Context)
  if (!value) throw new Error('SessionProvider is missing')
  return value
}
