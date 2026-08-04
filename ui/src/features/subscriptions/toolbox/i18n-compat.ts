import { useI18n } from '@/lib/i18n'
import en from './locales/en-US'
import zh from './locales/zh-CN'

type TranslationOptions = string | Record<string, unknown>

function lookup(root: unknown, path: string): unknown {
  return path.split('.').reduce<unknown>((value, key) => {
    if (!value || typeof value !== 'object') return undefined
    return (value as Record<string, unknown>)[key]
  }, root)
}

function interpolate(value: string, options?: TranslationOptions) {
  if (!options || typeof options === 'string') return value
  return value.replace(/{{\s*([^}\s]+)\s*}}/g, (_, key: string) => String(options[key] ?? ''))
}

export function useTranslation() {
  const { locale } = useI18n()
  const resource = locale === 'zh-CN' ? zh : en
  return {
    t(key: string, options?: TranslationOptions): string {
      const normalized = key.startsWith('proxy.') ? key.slice('proxy.'.length) : key
      const value = lookup(resource, normalized)
      if (typeof value === 'string') return interpolate(value, options)
      const common: Record<string, Record<string, string>> = {
        'zh-CN': { 'common.save': '保存', 'common.cancel': '取消', 'common.loading': '加载中', 'common.copy': '复制', 'common.close': '关闭', yes: '是', no: '否' },
        en: { 'common.save': 'Save', 'common.cancel': 'Cancel', 'common.loading': 'Loading', 'common.copy': 'Copy', 'common.close': 'Close', yes: 'Yes', no: 'No' },
      }
      return common[locale]?.[key] ?? key
    },
  }
}
