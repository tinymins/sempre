import { CodeBlock, type CodeBlockProps } from '@acme/components'
import { useI18n } from './i18n/provider'

export function ServerCodeBlock(props: Omit<CodeBlockProps, 'copyLabel' | 'copiedLabel' | 'copyErrorLabel'>) {
  const { t } = useI18n()
  return <CodeBlock {...props} copyLabel={t('common.copy')} copiedLabel={t('common.copied')} copyErrorLabel={t('common.copyFailed')} />
}
