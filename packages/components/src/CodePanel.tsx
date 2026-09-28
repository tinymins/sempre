import type { CSSProperties, ReactNode } from 'react'
import { cn } from './utils'

export interface CodePanelProps {
  children: ReactNode
  title?: ReactNode
  language?: string
  actions?: ReactNode
  className?: string
  bodyClassName?: string
  maxHeight?: CSSProperties['maxHeight']
  padded?: boolean
  scrollLabel?: string
}

export function CodePanel({
  children,
  title,
  language,
  actions,
  className,
  bodyClassName,
  maxHeight,
  padded = true,
  scrollLabel,
}: CodePanelProps) {
  const hasHeader = title != null || language != null || actions != null

  return <section className={cn('acme-code-panel', className)}>
    {hasHeader && <div className="acme-code-panel__header">
      <div className="acme-code-panel__heading">
        {title != null && <span className="acme-code-panel__title">{title}</span>}
        {language && <span className="acme-code-panel__language">{language}</span>}
      </div>
      {actions != null && <div className="acme-code-panel__actions">{actions}</div>}
    </div>}
    <div
      className={cn('acme-code-panel__body', !padded && 'acme-code-panel__body--flush', bodyClassName)}
      style={maxHeight == null ? undefined : { maxHeight }}
      tabIndex={maxHeight == null ? undefined : 0}
      aria-label={maxHeight == null ? undefined : scrollLabel}
    >
      {children}
    </div>
  </section>
}
