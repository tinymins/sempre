import { Modal, Pagination, Spin, Table } from '@acme/components'
import { useEffect, useState } from 'react'
import { subscriptionApi } from './api'
import type { AccessStats } from './diagnostic-types'
import type { Subscription } from './types'

export function SubscriptionStats({ subscription, onClose }: { subscription: Subscription | null; onClose: () => void }) {
  const [stats, setStats] = useState<AccessStats | null>(null)
  const [page, setPage] = useState(1)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  useEffect(() => {
    if (!subscription) return
    let active = true
    void subscriptionApi.stats(subscription.id, page).then((next) => { if (active) setStats(next) })
      .catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [subscription, page])

  return <Modal open={Boolean(subscription)} title={`访问统计 · ${subscription?.remark || '未命名配置集'}`} footer={null} onCancel={onClose} size="large">
    {loading ? <Spin size="large" /> : null}
    {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
    {stats ? <div className="space-y-5">
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <Metric label="累计访问" value={stats.totalAccesses} />
        <Metric label="今日访问" value={stats.todayAccess} />
        <Metric label="缓存节点" value={stats.cachedNodeCount} />
        <Metric label="最后访问" value={stats.lastAccessAt ? new Date(stats.lastAccessAt).toLocaleString() : '—'} />
      </div>
      <section><h3 className="mb-2 text-sm font-medium">按格式</h3>
        <div className="flex flex-wrap gap-2">{stats.accessByType.map((item) => <span key={item.type} className="rounded border border-[var(--border)] px-2 py-1 text-xs">{item.type}: {item.count}</span>)}</div>
      </section>
      <section><h3 className="mb-2 text-sm font-medium">最近访问</h3>
        <Table rowKey="id" dataSource={stats.recentAccesses} pagination={false} size="small" scroll={{ x: 620 }} columns={[
          { title: '时间', render: (_, item) => new Date(item.createdAt).toLocaleString() },
          { title: '格式', dataIndex: 'accessType' },
          { title: '节点', dataIndex: 'nodeCount' },
          { title: 'IP', dataIndex: 'ip' },
          { title: 'User-Agent', dataIndex: 'userAgent' },
        ]} />
        <Pagination className="mt-3" current={page} total={stats.recentAccessTotal} pageSize={20} onChange={setPage} />
      </section>
    </div> : null}
  </Modal>
}

function Metric({ label, value }: { label: string; value: string | number }) {
  return <div className="rounded-lg border border-[var(--border)] p-3"><p className="text-xs text-[var(--muted)]">{label}</p><strong className="mt-1 block text-lg">{value}</strong></div>
}
