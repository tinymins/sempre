import { useEffect, useMemo, useState, type ReactNode } from 'react'
import {
  ToastProvider,
  UIContext,
  type UIContextValue,
} from '@acme/components'

function currentTheme(): UIContextValue['theme'] {
  return document.documentElement.classList.contains('dark') ? 'dark' : 'light'
}

export function AcmeContentBoundary({ children }: { children: ReactNode }) {
  const [theme, setTheme] = useState<UIContextValue['theme']>(currentTheme)

  useEffect(() => {
    const root = document.documentElement
    const observer = new MutationObserver(() => setTheme(currentTheme()))
    observer.observe(root, { attributes: true, attributeFilter: ['class'] })
    return () => observer.disconnect()
  }, [])

  const ui = useMemo<UIContextValue>(
    () => ({ wallpaperUrl: null, theme, windowBlur: 0, windowOpacity: 100 }),
    [theme],
  )

  return (
    <div className="acme-content-scope min-h-0" data-acme-content-boundary>
      <UIContext.Provider value={ui}>
        <ToastProvider>{children}</ToastProvider>
      </UIContext.Provider>
    </div>
  )
}
