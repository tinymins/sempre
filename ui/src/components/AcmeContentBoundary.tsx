import { useEffect, useMemo, useState, type ReactNode, type RefObject } from 'react'
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
  const [portalElement, setPortalElement] = useState<HTMLDivElement | null>(null)
  const [theme, setTheme] = useState<UIContextValue['theme']>(currentTheme)
  const portalRef = useMemo<RefObject<HTMLElement | null>>(() => ({ current: portalElement }), [portalElement])

  useEffect(() => {
    const root = document.documentElement
    const observer = new MutationObserver(() => setTheme(currentTheme()))
    observer.observe(root, { attributes: true, attributeFilter: ['class'] })
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    if (!portalElement) return
    setActiveModalContainer(portalRef)
    return () => setActiveModalContainer(null)
  }, [portalElement, portalRef])

  const ui = useMemo<UIContextValue>(
    () => ({ wallpaperUrl: null, theme, windowBlur: 0, windowOpacity: 100 }),
    [theme],
  )

  return (
    <div className="acme-content-scope relative isolate min-h-0" data-acme-content-boundary>
      <div ref={setPortalElement} className="acme-portal-root pointer-events-none fixed inset-0 z-50" data-acme-portal-root />
      {portalElement ? (
        <UIContext.Provider value={ui}>
          <ModalContainerContext.Provider value={portalRef}>
            <ToastProvider>{children}</ToastProvider>
          </ModalContainerContext.Provider>
        </UIContext.Provider>
      ) : null}
    </div>
  )
}
