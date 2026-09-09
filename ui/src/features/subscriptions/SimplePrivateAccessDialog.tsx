import { Button, Modal } from '@acme/components'
import { ChevronRight } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useI18n } from '../../lib/i18n'
import { parseConfig } from './toolbox/PrivateAccessConfig'
import PrivateAccessEditor from './toolbox/PrivateAccessEditor'

export function SimplePrivateAccessDialog({ profileId, value, onChange }: { profileId: string; value: string; onChange: (value: string) => void }) {
  const { locale } = useI18n()
  const zh = locale === 'zh-CN'
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState(value)
  const config = useMemo(() => parseConfig(value), [value])
  const enabledConnectors = config.connectors.filter((connector) => connector.enabled).length
  const summary = config.connectors.length === 0
    ? (zh ? '未配置' : 'Not configured')
    : config.enabled
      ? (zh ? `已启用 · ${enabledConnectors} 个连接器` : `Enabled · ${enabledConnectors} connector${enabledConnectors === 1 ? '' : 's'}`)
      : (zh ? `已关闭 · ${config.connectors.length} 个连接器` : `Disabled · ${config.connectors.length} connector${config.connectors.length === 1 ? '' : 's'}`)

  const show = () => {
    setDraft(value)
    setOpen(true)
  }

  return <>
    <div className="mt-5 border-t border-[var(--border)] pt-2">
      <Button
        variant="unstyled"
        className="flex min-h-11 w-full items-center rounded-md px-2 text-left hover:bg-[var(--surface-hover)] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-emerald-500"
        onClick={show}
      >
        <span className="min-w-0 flex-1">
          <span className="text-sm font-medium text-[var(--text)]">{zh ? '内网访问' : 'Private Access'}</span>
          <span className="ml-2 text-xs text-[var(--muted)]">{zh ? '可选' : 'Optional'}</span>
        </span>
        <span className="text-xs text-[var(--muted)]">{summary}</span>
        <ChevronRight className="ml-2 size-4 shrink-0 text-[var(--muted)]" />
      </Button>
    </div>

    <Modal
      open={open}
      centered
      destroyOnClose
      title={zh ? '内网访问' : 'Private Access'}
      okText={zh ? '完成' : 'Done'}
      cancelText={zh ? '取消' : 'Cancel'}
      width="min(1080px, calc(100vw - 32px))"
      style={{ height: 'min(85dvh, 900px)' }}
      bodyStyle={{ flex: 1, minHeight: 0, overflowY: 'auto' }}
      onOk={() => {
        onChange(draft)
        setOpen(false)
      }}
      onCancel={() => setOpen(false)}
    >
      <PrivateAccessEditor profileId={profileId} value={draft} onChange={setDraft} />
    </Modal>
  </>
}
