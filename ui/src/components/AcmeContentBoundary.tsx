import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import {
  ModalContainerContext,
  setActiveModalContainer,
  ToastProvider,
  UIContext,
  type UIContextValue,
} from '@acme/components'

function currentTheme(): UIContextValue['theme'] {
  return document.documentElement.classList.contains('dark') ? 'dark' : 'light'
}

export function AcmeContentBoundary({ children }: { children: ReactNode }) {
  const portalRef = useRef<HTMLDivElement | null>(null)
  const [portalReady, setPortalReady] = useState(false)
  const [theme, setTheme] = useState<UIContextValue['theme']>(currentTheme)

  useLayoutEffect(() => {
    if (portalRef.current) setPortalReady(true)
  }, [])

  useEffect(() => {
    const root = document.documentElement
    const observer = new MutationObserver(() => setTheme(currentTheme()))
    observer.observe(root, { attributes: true, attributeFilter: ['class'] })
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    setActiveModalContainer(portalRef)
    return () => setActiveModalContainer(null)
  }, [])

  const ui = useMemo<UIContextValue>(
    () => ({ wallpaperUrl: null, theme, windowBlur: 0, windowOpacity: 100 }),
    [theme],
  )

  return (
    <div className="acme-content-scope relative isolate min-h-0" data-acme-content-boundary>
      <div ref={portalRef} className="acme-portal-root pointer-events-none fixed inset-0 z-50" data-acme-portal-root />
      {portalReady ? (
        <UIContext.Provider value={ui}>
          <ModalContainerContext.Provider value={portalRef}>
            <ToastProvider>{children}</ToastProvider>
          </ModalContainerContext.Provider>
        </UIContext.Provider>
      ) : null}
    </div>
  )
}
