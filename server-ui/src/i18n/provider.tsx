import { createContext, useContext, useLayoutEffect, useMemo, useState, type ReactNode } from 'react'
import { zhCN, type MessageKey } from './zh-CN'
import { zhTW } from './zh-TW'
import { enUS } from './en-US'
import { jaJP } from './ja-JP'
import { deDE } from './de-DE'

export type Locale = 'zh-CN' | 'zh-TW' | 'en-US' | 'ja-JP' | 'de-DE'
export type LanguageMode = 'auto' | Locale

const translations: Record<Locale, Record<MessageKey, string>> = {
  'zh-CN': zhCN, 'zh-TW': zhTW, 'en-US': enUS, 'ja-JP': jaJP, 'de-DE': deDE,
}
const supported: Locale[] = ['zh-CN', 'zh-TW', 'en-US', 'ja-JP', 'de-DE']

function browserLocale(): Locale {
  for (const candidate of navigator.languages.length ? navigator.languages : [navigator.language]) {
    const exact = supported.find((locale) => locale.toLowerCase() === candidate.toLowerCase())
    if (exact) return exact
    if (candidate.toLowerCase().startsWith('zh')) return /hant|tw|hk|mo/i.test(candidate) ? 'zh-TW' : 'zh-CN'
    const base = supported.find((locale) => locale.split('-')[0] === candidate.split('-')[0])
    if (base) return base
  }
  return 'zh-CN'
}

function translate(locale: Locale, key: MessageKey, values: Record<string, string | number> = {}): string {
  return Object.entries(values).reduce<string>((message, [name, replacement]) => message.replaceAll(`{${name}}`, String(replacement)), translations[locale][key])
}

export function translateCurrent(key: MessageKey): string {
  const current = document.documentElement.lang
  return translate(supported.find((locale) => locale === current) ?? browserLocale(), key)
}

interface I18nContextValue {
  mode: LanguageMode
  locale: Locale
  setMode: (mode: LanguageMode) => void
  t: (key: MessageKey, values?: Record<string, string | number>) => string
  date: (value: string | Date, options?: Intl.DateTimeFormatOptions) => string
  number: (value: number) => string
}

const Context = createContext<I18nContextValue | null>(null)

export function I18nProvider({ children }: { children: ReactNode }) {
  const [mode, setMode] = useState<LanguageMode>('auto')
  const locale = mode === 'auto' ? browserLocale() : mode
  useLayoutEffect(() => { document.documentElement.lang = locale }, [locale])
  const value = useMemo<I18nContextValue>(() => ({
    mode, locale, setMode,
    t: (key, values = {}) => translate(locale, key, values),
    date: (input, options = { dateStyle: 'short', timeStyle: 'short' }) => new Intl.DateTimeFormat(locale, options).format(new Date(input)),
    number: (input) => new Intl.NumberFormat(locale).format(input),
  }), [mode, locale])
  return <Context.Provider value={value}>{children}</Context.Provider>
}

export function useI18n(): I18nContextValue {
  const value = useContext(Context)
  if (!value) throw new Error('I18nProvider is missing')
  return value
}
