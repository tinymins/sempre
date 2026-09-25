import type { ServerUser } from '../server-api'

const palette = {
  emerald: ['#10b981', '#059669', '16,185,129'],
  amber: ['#f59e0b', '#d97706', '245,158,11'],
  rose: ['#f43f5e', '#e11d48', '244,63,94'],
  violet: ['#8b5cf6', '#7c3aed', '139,92,246'],
  blue: ['#3b82f6', '#2563eb', '59,130,246'],
  cyan: ['#06b6d4', '#0891b2', '6,182,212'],
} as const

export function applyAppearance(user: ServerUser | null): () => void {
  const root = document.documentElement
  const settings = user?.settings ?? {}
  const mode = settings.themeMode
  const media = window.matchMedia?.('(prefers-color-scheme: dark)')
  const accent = settings.accentColor
  const [base, hover, rgb] = palette[typeof accent === 'string' && accent in palette ? accent as keyof typeof palette : 'emerald']
  const sync = () => {
    const dark = mode === 'dark' || (mode !== 'light' && media?.matches === true)
    root.classList.toggle('dark', dark)
    root.style.setProperty('--accent-subtle', `rgba(${rgb}, ${dark ? '0.08' : '0.1'})`)
  }
  sync()
  if (mode !== 'dark' && mode !== 'light') media?.addEventListener('change', sync)
  root.style.setProperty('--accent', base)
  root.style.setProperty('--accent-hover', hover)
  root.style.setProperty('--accent-subtle-hover', `rgba(${rgb}, 0.18)`)
  root.style.setProperty('--accent-muted', `rgba(${rgb}, 0.5)`)
  root.style.setProperty('--accent-text', base)
  return () => { media?.removeEventListener('change', sync) }
}
