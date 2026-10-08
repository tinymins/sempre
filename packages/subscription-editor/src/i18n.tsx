import { createContext, useContext, type ReactNode } from 'react'
import zh from './messages/zh-CN'
import en from './messages/en-US'
import tw from './messages/zh-TW'
import ja from './messages/ja-JP'
import de from './messages/de-DE'

const messages: Record<string, Record<string, string>> = { 'zh-CN': zh, 'en-US': en, en, 'zh-TW': tw, 'ja-JP': ja, 'de-DE': de }
const LocaleContext = createContext({ locale: 'en-US', messages: {} as Record<string, string> })

export function EditorProvider({ locale, messages = {}, children }: { locale: string; messages?: Record<string, string>; children: ReactNode }) {
  return <LocaleContext.Provider value={{ locale, messages }}>{children}</LocaleContext.Provider>
}

export function useEditorI18n() {
  const { locale, messages: overrides } = useContext(LocaleContext)
  const dictionary = messages[locale] ?? en
  return {
    t: (key: string, values: Record<string, string | number> = {}) =>
      Object.entries(values).reduce((text, [name, value]) => text.replaceAll(`{{${name}}}`, String(value)).replaceAll(`{${name}}`, String(value)), overrides[key] ?? dictionary[key] ?? key),
    number: (value: number) => new Intl.NumberFormat(locale).format(value),
  }
}
