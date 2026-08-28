import { createContext, useContext, useEffect, useState, type ReactNode } from 'react'

export type Theme = 'system' | 'light' | 'dark'

const THEME_KEY = 'sempre.theme'
const ThemeContext = createContext<{ theme: Theme; setTheme: (theme: Theme) => void } | null>(null)

function storedTheme(): Theme {
  const theme = localStorage.getItem(THEME_KEY)
  return theme === 'light' || theme === 'dark' ? theme : 'system'
}

function applyTheme(theme: Theme, systemDark = matchMedia('(prefers-color-scheme: dark)').matches) {
  document.documentElement.classList.toggle('dark', theme === 'dark' || (theme === 'system' && systemDark))
}

export function initializeTheme() {
  applyTheme(storedTheme())
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setTheme] = useState<Theme>(storedTheme)

  useEffect(() => {
    localStorage.setItem(THEME_KEY, theme)
    const systemTheme = matchMedia('(prefers-color-scheme: dark)')
    const syncTheme = () => applyTheme(theme, systemTheme.matches)
    syncTheme()
    if (theme !== 'system') return
    systemTheme.addEventListener('change', syncTheme)
    return () => systemTheme.removeEventListener('change', syncTheme)
  }, [theme])

  return <ThemeContext value={{ theme, setTheme }}>{children}</ThemeContext>
}

export function useTheme() {
  const value = useContext(ThemeContext)
  if (!value) throw new Error('useTheme must be used within ThemeProvider')
  return value
}
