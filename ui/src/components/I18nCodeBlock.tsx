import { CodeBlock as AcmeCodeBlock, type CodeBlockProps } from '@acme/components'
import { useI18n } from '../lib/i18n'

export function I18nCodeBlock(props: CodeBlockProps) {
  const { locale } = useI18n()
  return <AcmeCodeBlock
    copyLabel={locale === 'zh-CN' ? '复制' : 'Copy'}
    copiedLabel={locale === 'zh-CN' ? '已复制' : 'Copied'}
    copyErrorLabel={locale === 'zh-CN' ? '复制失败' : 'Copy failed'}
    {...props}
  />
}
