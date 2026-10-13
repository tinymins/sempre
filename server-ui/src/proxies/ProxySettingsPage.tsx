import { Tabs } from '@acme/components'
import { useState } from 'react'
import { useI18n } from '../i18n/provider'
import { ProxyNodesPanel } from './ProxyNodesPanel'
import { RelayEgressPanel } from './RelayEgressPanel'

export function ProxySettingsPage({ canManageRules }: { canManageRules: boolean }) {
  const { t } = useI18n()
  const [tab, setTab] = useState('nodes')
  return <section className="min-w-0 space-y-5">
    <div><h1 className="text-xl font-semibold">{t('proxies.title')}</h1><p className="mt-1 text-sm text-[var(--muted)]">{t('proxies.description')}</p></div>
    <Tabs destroyInactiveTabPane activeKey={tab} onChange={setTab} items={[
      { key: 'nodes', label: t('proxies.nodesTab'), children: <ProxyNodesPanel /> },
      ...(canManageRules ? [{ key: 'rules', label: t('proxies.rulesTab'), children: <RelayEgressPanel /> }] : []),
    ]} />
  </section>
}
