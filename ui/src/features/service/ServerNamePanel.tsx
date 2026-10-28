import { useState } from 'react'
import { useMutation } from '@tanstack/react-query'
import { Button, Input, useToast } from '@acme/components'
import { MonitorCog } from 'lucide-react'
import { Card, Field } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { useUIMode } from '../../lib/uiMode'
import { useSession } from '../../lib/session'

export function ServerNamePanel() {
  const { locale, t } = useI18n()
  const { session } = useSession()
  const toast = useToast()
  const settings = useUIMode()
  const [draft, setDraft] = useState<string | null>(null)
  const zh = locale === 'zh-CN'
  const name = draft ?? settings.friendlyName
  const update = useMutation({
    mutationFn: () => settings.setFriendlyName(name),
    onSuccess: () => { setDraft(null); toast.success(t('operationDone')) },
    onError: (error) => toast.error(error.message),
  })
  return <Card className="min-w-0 p-4 md:p-5">
    <div className="mb-5 flex items-center gap-2"><MonitorCog size={18} className="text-emerald-600" /><h2 className="text-sm font-semibold">{zh ? '服务器名称' : 'Server name'}</h2></div>
    <Field label={zh ? '服务器名称（Friendly name）' : 'Friendly name'} hint={zh ? '仅用于展示。留空时，浏览器标题显示访问服务器的 IP 或域名。' : 'For display only. Leave blank to show the server IP or domain in the browser title.'}>
      <div className="flex flex-wrap gap-2">
        <Input className="min-w-0 flex-1 basis-56" value={name} placeholder={new URL(session!.baseURL).hostname} disabled={!settings.data || update.isPending} onChange={(event) => setDraft(event.target.value)} />
        <Button variant="primary" disabled={!settings.data || name.trim() === settings.friendlyName} loading={update.isPending} onClick={() => update.mutate()}>{t('save')}</Button>
      </div>
    </Field>
  </Card>
}
