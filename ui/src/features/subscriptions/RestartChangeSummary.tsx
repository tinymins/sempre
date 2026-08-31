import { ArrowRight, Boxes, FileSliders, Layers3 } from 'lucide-react'
import type { ReactNode } from 'react'
import { useI18n } from '../../lib/i18n'

export type RuntimePendingChange =
  | { type: 'core'; previous?: string; current: string }
  | { type: 'profile'; previous?: string; current: string }
  | { type: 'configuration'; fields: string[]; previous_revision?: number; current_revision?: number }

const fieldKeys = {
  sources: 'changeFieldSources',
  subscription_content: 'changeFieldSubscriptionContent',
  nodes: 'changeFieldNodes',
  groups: 'changeFieldGroups',
  rules: 'changeFieldRules',
  rule_providers: 'changeFieldRuleProviders',
  filters: 'changeFieldFilters',
  dns: 'changeFieldDNS',
  private_access: 'changeFieldPrivateAccess',
  local_proxy: 'changeFieldLocalProxy',
  transparent_proxy: 'changeFieldTransparentProxy',
  management_api: 'changeFieldManagementAPI',
  advanced: 'changeFieldAdvanced',
  manual_configuration: 'changeFieldManualConfiguration',
} as const

export function RestartChangeSummary({ detail, changes = [] }: { detail: string; changes?: RuntimePendingChange[] }) {
  const { locale, t } = useI18n()
  if (!changes.length) return <p>{detail}</p>
  const fieldList = new Intl.ListFormat(locale === 'zh-CN' ? 'zh-CN' : 'en', { style: 'long', type: 'conjunction' })
  return (
    <div className="space-y-4">
      <p>{detail}</p>
      <div className="rounded-lg border border-amber-500/30 bg-amber-500/6 p-3">
        <p className="mb-2 font-medium text-[var(--text)]">{t('restartChangesTitle')}</p>
        <div className="space-y-2.5">
          {changes.map((change, index) => (
            <ChangeRow
              key={`${change.type}-${index}`}
              icon={change.type === 'core' ? <Boxes size={15} /> : change.type === 'profile' ? <Layers3 size={15} /> : <FileSliders size={15} />}
              label={t(change.type === 'core' ? 'changeCore' : change.type === 'profile' ? 'changeProfile' : 'changeConfiguration')}
            >
              {change.type === 'configuration'
                ? change.fields.length
                  ? fieldList.format(change.fields.map((field) => t(fieldKeys[field as keyof typeof fieldKeys] ?? 'changeFieldUnknown')))
                  : t('changeFieldUnknown')
                : <Transition previous={change.previous} current={change.current} fallback={t('changeNone')} />}
            </ChangeRow>
          ))}
        </div>
      </div>
    </div>
  )
}

function ChangeRow({ icon, label, children }: { icon: ReactNode; label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-2">
      <span className="mt-0.5 text-amber-600 dark:text-amber-400">{icon}</span>
      <div>
        <p className="text-xs font-medium text-[var(--muted)]">{label}</p>
        <div className="mt-0.5 break-words text-sm font-medium text-[var(--text)]">{children}</div>
      </div>
    </div>
  )
}

function Transition({ previous, current, fallback }: { previous?: string; current: string; fallback: string }) {
  return (
    <span className="inline-flex flex-wrap items-center gap-1.5">
      <span>{previous || fallback}</span>
      <ArrowRight aria-hidden="true" size={14} className="text-[var(--muted)]" />
      <span>{current}</span>
    </span>
  )
}
