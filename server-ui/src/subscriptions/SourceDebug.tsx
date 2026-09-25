import { Button, Modal, Select, Spin, Table } from '@acme/components'
import { useState } from 'react'
import { subscriptionApi } from './api'
import type { SourceDebugResult } from './diagnostic-types'
import type { SubscriptionSource } from './types'

export function SourceDebug({ source, onClose }: { source: SubscriptionSource; onClose: () => void }) {
  const [mode, setMode] = useState<'bypass-cache' | 'production'>('bypass-cache')
  const [result, setResult] = useState<SourceDebugResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const run = async () => {
    setLoading(true)
    setError('')
    setResult(null)
    try {
      setResult(await subscriptionApi.debugSource(source, mode))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setLoading(false)
    }
  }
  return <Modal open title={`来源调试 · ${source.remark || source.url}`} size="large" footer={null} onCancel={onClose}>
    <div className="space-y-4">
      <p className="break-all text-xs text-[var(--muted)]">{source.url}</p>
      <div className="flex items-end gap-2"><label className="min-w-48 flex-1 space-y-1 text-sm">抓取方式
        <Select value={mode} options={[{ value: 'bypass-cache', label: '绕过缓存' }, { value: 'production', label: '按正式配置' }]} onChange={(next) => setMode(next as typeof mode)} className="w-full" />
      </label><Button variant="primary" loading={loading} onClick={() => void run()}>开始调试</Button></div>
      {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
      {loading ? <Spin /> : null}
      {result ? <>
        <div className="grid grid-cols-2 gap-2 text-sm md:grid-cols-4">
          <span>HTTP {result.status}</span><span>缓存：{result.cacheState}</span>
          <span>{result.elapsedMs} ms</span><span>{result.bodyBytes} 字节</span>
        </div>
        <p className="text-xs text-[var(--muted)]">User-Agent: {result.ua} · 解析节点 {result.nodeCount}</p>
        {result.warning ? <p className="text-xs text-amber-700">{result.warning}</p> : null}
        {result.diagnostics != null ? <details><summary className="cursor-pointer text-sm">抓取与解析诊断</summary><pre className="max-h-60 overflow-auto text-xs">{JSON.stringify(result.diagnostics, null, 2)}</pre></details> : null}
        <Table rowKey={(_, index) => String(index)} dataSource={result.nodes} pagination={false} size="small" scroll={{ x: 500 }} columns={[
          { title: '名称', dataIndex: 'name' }, { title: '协议', dataIndex: 'type' },
          { title: '服务器', render: (_, node) => `${node.server}:${node.port}` },
        ]} />
      </> : null}
    </div>
  </Modal>
}
