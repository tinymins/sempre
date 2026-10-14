import { useEffect, useMemo, useState, type ReactNode } from 'react'
import {
  ToastProvider,
  UIContext,
  type UIContextValue,
} from '@acme/components'

function currentTheme(): UIContextValue['theme'] {
  return document.documentElement.classList.contains('dark') ? 'dark' : 'light'
}

export function AcmeProviders({ children }: { children: ReactNode }) {
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
    <UIContext.Provider value={ui}>
      <ToastProvider>{children}</ToastProvider>
    </UIContext.Provider>
  )
}

export function AcmeContentScope({ children }: { children: ReactNode }) {
  return <div className="acme-content-scope min-h-0" data-acme-content-boundary>{children}</div>
}

export function AcmeContentBoundary({ children }: { children: ReactNode }) {
  return <AcmeContentScope><AcmeProviders>{children}</AcmeProviders></AcmeContentScope>
}
