import { useState } from 'react'
import { Download, ShieldCheck } from 'lucide-react'
import { Button, Tabs } from '@acme/components'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { AppObservation } from './leakDetection/AppObservation'
import { QuickCheck } from './leakDetection/QuickCheck'
import { type LeakMode, useLeakDetection } from './leakDetection/useLeakDetection'

export function LeakDetection() {
  const { locale } = useI18n()
  const { session } = useSession()
  const zh = locale === 'zh-CN'
  const check = useLeakDetection(session, zh)
  const [mode, setMode] = useState<LeakMode>('quick')

  return <div className="min-w-0 space-y-6">
    <header className="flex min-w-0 flex-wrap items-start justify-between gap-3">
      <div className="min-w-0 space-y-2">
        <div className="flex min-w-0 items-center gap-2"><ShieldCheck size={23} className="shrink-0 text-[var(--muted)]" /><h1 className="text-2xl font-semibold">{zh ? '泄漏检测' : 'Leak detection'}</h1></div>
        <p className="text-sm text-[var(--muted)]">{zh ? '检查真实地址是否经代理以外的路径暴露。' : 'Check whether your real address is exposed outside the proxy.'}</p>
      </div>
      {!check.running && (check.report || check.browser) && <Button icon={<Download size={14} />} onClick={check.exportReport}>{zh ? '导出报告' : 'Export report'}</Button>}
    </header>
    <Tabs type="segment" className="min-w-0" activeKey={mode} onChange={(key) => setMode(key as LeakMode)} items={[
      { key: 'quick', label: zh ? '快速检测' : 'Quick check', disabled: check.running && mode !== 'quick', children: <div className="min-w-0 pt-5"><QuickCheck check={check} zh={zh} /></div> },
      { key: 'observe', label: zh ? '应用观测' : 'Application observation', disabled: check.running && mode !== 'observe', children: <div className="min-w-0 pt-5"><AppObservation check={check} zh={zh} /></div> },
    ]} />
  </div>
}
