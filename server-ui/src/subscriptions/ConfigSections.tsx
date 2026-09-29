import { Select } from '@acme/components'
import type { SubscriptionDraft, UserBrief } from './types'
import { useI18n } from '../i18n/provider'

export function AuthorizationFields({ draft, users, canManageAuthorization, update }: { draft: SubscriptionDraft; users: UserBrief[] | null; canManageAuthorization: boolean; update: (patch: Partial<SubscriptionDraft>) => void }) {
  const { t } = useI18n()
  return (
    <div className="space-y-4">
      <label className="block space-y-1 text-sm">{t('editor.authorizedUsers')}
        <Select mode="multiple" value={draft.authorizedUserIds} disabled={!canManageAuthorization || users === null} options={users?.map((user) => ({ value: user.id, label: `${user.name} (${user.email})` })) ?? []} onChange={(next) => update({ authorizedUserIds: next as string[] })} showSearch placeholder={t('editor.authorizedUsers')} className="w-full" />
      </label>
      {!canManageAuthorization ? <p className="text-xs text-[var(--muted)]">{t('editor.ownerOnly')}</p> : null}
    </div>
  )
}
