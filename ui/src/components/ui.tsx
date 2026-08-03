import { useState, type ButtonHTMLAttributes, type HTMLAttributes, type InputHTMLAttributes, type ReactNode } from 'react'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '../lib/cn'

const buttonVariants = cva(
  'inline-flex h-9 shrink-0 items-center justify-center gap-2 rounded-md px-3 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 disabled:pointer-events-none disabled:opacity-50',
  {
    variants: {
      variant: {
        primary: 'bg-emerald-600 text-white hover:bg-emerald-700',
        secondary: 'border border-[var(--border)] bg-[var(--surface)] text-[var(--text)] hover:bg-[var(--surface-hover)]',
        ghost: 'text-[var(--muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]',
        danger: 'bg-red-600 text-white hover:bg-red-700',
      },
      size: {
        normal: 'h-9 px-3',
        icon: 'size-9 px-0',
        small: 'h-8 px-2 text-xs',
      },
    },
    defaultVariants: { variant: 'secondary', size: 'normal' },
  },
)

export interface ButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {}

export function Button({ className, variant, size, ...props }: ButtonProps) {
  return <button className={cn(buttonVariants({ variant, size }), className)} {...props} />
}

export function Input({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={cn(
        'h-9 w-full rounded-md border border-[var(--border)] bg-[var(--surface)] px-3 text-sm text-[var(--text)] outline-none placeholder:text-[var(--muted)] focus:border-emerald-500 focus:ring-2 focus:ring-emerald-500/20',
        className,
      )}
      {...props}
    />
  )
}

export function Card({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('rounded-lg border border-[var(--border)] bg-[var(--surface)]', className)} {...props} />
}

export function Badge({ children, tone = 'neutral' }: { children: ReactNode; tone?: 'neutral' | 'success' | 'warning' | 'danger' | 'info' }) {
  const tones = {
    neutral: 'bg-[var(--surface-hover)] text-[var(--muted)]',
    success: 'bg-emerald-500/12 text-emerald-600 dark:text-emerald-400',
    warning: 'bg-amber-500/14 text-amber-700 dark:text-amber-400',
    danger: 'bg-red-500/12 text-red-600 dark:text-red-400',
    info: 'bg-cyan-500/12 text-cyan-700 dark:text-cyan-400',
  }
  return <span className={cn('inline-flex h-6 items-center rounded px-2 text-xs font-medium', tones[tone])}>{children}</span>
}

export function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="grid gap-1.5 text-sm font-medium text-[var(--text)]">
      <span>{label}</span>
      {children}
      {hint ? <span className="text-xs font-normal text-[var(--muted)]">{hint}</span> : null}
    </label>
  )
}

export function EmptyState({ title, detail, action }: { title: string; detail: string; action?: ReactNode }) {
  return (
    <div className="flex min-h-52 flex-col items-center justify-center border-y border-dashed border-[var(--border)] px-6 text-center">
      <p className="text-sm font-semibold text-[var(--text)]">{title}</p>
      <p className="mt-1 max-w-md text-sm text-[var(--muted)]">{detail}</p>
      {action ? <div className="mt-4">{action}</div> : null}
    </div>
  )
}

export function Spinner() {
  return <span className="inline-block size-4 animate-spin rounded-full border-2 border-current border-r-transparent" aria-label="loading" />
}

export function PageTitle({ title, detail, children }: { title: string; detail?: string; children?: ReactNode }) {
  return <div className="flex min-h-10 items-start justify-between gap-4"><div><h1 className="text-xl font-semibold">{title}</h1>{detail ? <p className="mt-1 text-sm text-[var(--muted)]">{detail}</p> : null}</div>{children}</div>
}

export function ConfirmDialog({
  open,
  title,
  detail,
  confirmLabel,
  cancelLabel,
  acknowledgement,
  pending = false,
  onCancel,
  onConfirm,
}: {
  open: boolean
  title: string
  detail: string
  confirmLabel: string
  cancelLabel: string
  acknowledgement?: string
  pending?: boolean
  onCancel: () => void
  onConfirm: () => void
}) {
  if (!open) return null
  return <ConfirmDialogContent title={title} detail={detail} confirmLabel={confirmLabel} cancelLabel={cancelLabel} acknowledgement={acknowledgement} pending={pending} onCancel={onCancel} onConfirm={onConfirm} />
}

function ConfirmDialogContent({
  title,
  detail,
  confirmLabel,
  cancelLabel,
  acknowledgement,
  pending = false,
  onCancel,
  onConfirm,
}: {
  title: string
  detail: string
  confirmLabel: string
  cancelLabel: string
  acknowledgement?: string
  pending?: boolean
  onCancel: () => void
  onConfirm: () => void
}) {
  const [acknowledged, setAcknowledged] = useState(false)
  return <div className="fixed inset-0 z-50 grid place-items-center bg-black/45 p-4" onMouseDown={(event) => { if (event.target === event.currentTarget && !pending) onCancel() }}>
    <div role="dialog" aria-modal="true" aria-labelledby="confirm-dialog-title" className="w-full max-w-md rounded-lg border border-[var(--border)] bg-[var(--surface)] p-5 shadow-2xl">
      <h2 id="confirm-dialog-title" className="text-base font-semibold">{title}</h2>
      <p className="mt-2 text-sm leading-6 text-[var(--muted)]">{detail}</p>
      {acknowledgement ? <label className="mt-4 flex items-start gap-3 rounded-md border border-amber-500/40 bg-amber-500/8 p-3 text-sm leading-5"><input className="mt-0.5 size-4 shrink-0 accent-red-600" type="checkbox" checked={acknowledged} onChange={(event) => setAcknowledged(event.target.checked)} /><span>{acknowledgement}</span></label> : null}
      <div className="mt-5 flex justify-end gap-2"><Button disabled={pending} onClick={onCancel}>{cancelLabel}</Button><Button variant="danger" disabled={pending || Boolean(acknowledgement && !acknowledged)} onClick={onConfirm}>{pending ? <Spinner /> : null}{confirmLabel}</Button></div>
    </div>
  </div>
}
