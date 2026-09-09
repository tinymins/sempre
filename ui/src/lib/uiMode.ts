import { useSyncExternalStore } from 'react'
import { useSession } from './session'

export type LocalUIMode = 'simple' | 'advanced'

const CHANGE_EVENT = 'sempre-ui-mode-change'

function storageKey(baseURL: string) {
  return `sempre.ui-mode:${baseURL}`
}

export function useLocalUIMode() {
  const { session } = useSession()
  const key = storageKey(session?.baseURL ?? 'local')
  const mode = useSyncExternalStore(
    (onChange) => {
      window.addEventListener(CHANGE_EVENT, onChange)
      window.addEventListener('storage', onChange)
      return () => {
        window.removeEventListener(CHANGE_EVENT, onChange)
        window.removeEventListener('storage', onChange)
      }
    },
    () => localStorage.getItem(key) === 'simple' ? 'simple' : 'advanced',
  )

  return {
    mode,
    setMode(next: LocalUIMode) {
      localStorage.setItem(key, next)
      window.dispatchEvent(new Event(CHANGE_EVENT))
    },
  }
}
