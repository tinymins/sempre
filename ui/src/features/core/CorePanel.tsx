import { useEffect, useState } from 'react'
import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query'
import { Download, Package, RefreshCw, Trash2, X } from 'lucide-react'
import { Table, type TableColumn } from '@acme/components'
import { api } from '../../lib/api'
import { compactHash, formatDate } from '../../lib/format'
import { useI18n } from '../../lib/i18n'
import { useSession } from '../../lib/session'
import { compareDate, compareText } from '../../lib/sort'
import type { CoreDownloadTask, CoreInstallation, CoresResponse, ManagedRuntimeStatus, SystemStatus } from '../../lib/types'
import { Badge, Button, Card, ConfirmDialog, Field, Input } from '../../components/ui'

type ChangeResult = { NeedsRestart?: boolean; changes?: ChangeResult[] }
const taskKey = ['cores', 'download-task']

export function CorePanel() {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [reference, setReference] = useState('sing-box@stable')
  const [notice, setNotice] = useState('')
  const [cancelTask, setCancelTask] = useState<CoreDownloadTask | null>(null)
  const cores = useQuery({ queryKey: ['cores'], queryFn: () => api<CoresResponse>(session!, '/cores') })
  const tasks = useQuery({
    queryKey: taskKey,
    queryFn: () => api<{ task: CoreDownloadTask | null }>(session!, '/cores/download'),
    refetchInterval: (query) => query.state.data?.task?.state === 'running' ? 500 : 3000,
    refetchIntervalInBackground: true,
  })
  const task = tasks.data?.task
  const action = useMutation({
    mutationFn: ({ operation, value }: { operation: string; value?: string }) => api<ChangeResult>(session!, `/cores/${operation}`, { method: 'POST', body: JSON.stringify({ reference: value || '' }) }),
    onSuccess: (result) => { setNotice(changeNotice(result, queryClient, t('operationDone'), t('changeDeferred'))); refreshCoreQueries(queryClient) },
    onError: (error) => setNotice(error.message),
  })
  const download = useMutation({
    mutationFn: (operation: 'install' | 'update') => api<{ task: CoreDownloadTask }>(session!, `/cores/${operation}`, { method: 'POST', body: JSON.stringify({ reference }) }),
    onSuccess: (result) => { setNotice(''); queryClient.setQueryData(taskKey, result) },
    onError: (error) => setNotice(error.message),
  })
  const removeTask = useMutation({
    mutationFn: (id: string) => api<{ task: null }>(session!, `/cores/download?id=${encodeURIComponent(id)}`, { method: 'DELETE' }),
    onSuccess: () => { queryClient.setQueryData(taskKey, { task: null }); setCancelTask(null) },
    onError: (error) => setNotice(error.message),
  })

  useEffect(() => {
    if (!task || task.state !== 'succeeded') return
    refreshCoreQueries(queryClient)
    removeTask.mutate(task.id)
  }, [queryClient, task?.id, task?.state]) // eslint-disable-line react-hooks/exhaustive-deps

  const installedColumns: Array<TableColumn<CoreInstallation>> = [
    { title: t('core'), dataIndex: 'core', sorter: (left, right) => compareText(left.core, right.core), render: (value) => <span className="font-medium">{value}</span> },
    { title: t('repository'), dataIndex: 'repository', sorter: (left, right) => compareText(left.repository, right.repository), render: (_value, item) => <div className="flex items-center gap-2"><Badge tone={item.official ? 'success' : 'warning'}>{item.official ? t('official') : t('custom')}</Badge><span className="font-mono text-xs text-[var(--muted)]">{item.repository}</span></div> },
    { title: t('version'), dataIndex: 'version', sorter: (left, right) => compareText(left.version, right.version), render: (value) => <span className="font-mono text-xs">{value}</span> },
    { title: t('channel'), dataIndex: 'channels', sorter: (left, right) => compareText(left.channels.join(' '), right.channels.join(' ')), render: (value) => (value as string[]).map((channel) => <Badge key={channel}>{channel}</Badge>) },
    { title: t('details'), key: 'details', sorter: (left, right) => compareDate(left.installation.installed_at, right.installation.installed_at), render: (_value, item) => <span className="text-xs text-[var(--muted)]">{compactHash(item.installation.digest)} · {formatDate(item.installation.installed_at)}</span> },
    { title: '', key: 'actions', width: 192, render: (_value, item) => { const selected = isSelectedCore(cores.data, item); return <div className="flex justify-end gap-2">{selected ? <Button size="small" disabled>{t('currentUse')}</Button> : <Button size="small" onClick={() => action.mutate({ operation: 'use', value: item.reference })}>{t('use')}</Button>}<Button size="icon" variant="ghost" title={t('remove')} disabled={selected} onClick={() => action.mutate({ operation: 'remove', value: item.reference })}><Trash2 size={15} /></Button></div> } },
  ]
  const taskColumns: Array<TableColumn<CoreDownloadTask>> = [
    { title: t('reference'), dataIndex: 'reference', render: (value) => <span className="font-mono text-xs">{value}</span> },
    { title: t('status'), dataIndex: 'stage', width: 150, render: (_value, item) => <Badge tone={taskTone(item)}>{taskStage(item, t)}</Badge> },
    { title: t('downloadProgress'), key: 'progress', width: 360, render: (_value, item) => <DownloadProgress task={item} /> },
    { title: '', key: 'actions', width: 130, render: (_value, item) => item.state === 'running' ? <Button size="small" variant="danger" onClick={() => setCancelTask(item)}><X size={14} />{t('cancelDownload')}</Button> : <Button size="small" onClick={() => removeTask.mutate(item.id)}>{t('clearDownload')}</Button> },
  ]
  const busy = task?.state === 'running' || download.isPending
  const displayedNotice = task?.state === 'failed' ? task.error || t('operationFailed') : notice

  return <Card className="min-w-0 p-4 md:p-5">
    <div className="mb-5 flex items-center gap-2"><span className="text-emerald-600"><Package size={18} /></span><h2 className="text-sm font-semibold">{t('core')}</h2></div>
    {displayedNotice ? <div className="mb-4 border-l-2 border-emerald-500 bg-emerald-500/8 px-3 py-2 text-sm">{displayedNotice}</div> : null}
    <div className="grid gap-4 border-b border-[var(--border)] pb-6 md:grid-cols-[minmax(0,1fr)_auto_auto]"><Field label={t('reference')}><><Input list="supported-core-references" value={reference} onChange={(event) => setReference(event.target.value)} placeholder="mihomo@stable" /><datalist id="supported-core-references">{cores.data?.supported.map((core) => <option key={core} value={`${core}@stable`} />)}</datalist></></Field><Button className="self-end" variant="primary" disabled={busy} onClick={() => download.mutate('install')}><Download size={16} />{t('install')}</Button><Button className="self-end" disabled={busy} onClick={() => download.mutate('update')}><RefreshCw size={16} />{t('update')}</Button></div>
    {task ? <><h3 className="mt-6 text-sm font-semibold">{t('downloadTasks')}</h3><Table<CoreDownloadTask> className="mt-3" rowKey="id" pagination={false} columns={taskColumns} dataSource={[task]} scroll={{ x: 760 }} /></> : null}
    <h3 className="mt-6 text-sm font-semibold">{t('installedVersions')}</h3>
    <Table<CoreInstallation> className="mt-3" rowKey="reference" loading={cores.isLoading} pagination={false} columns={installedColumns} dataSource={cores.data?.installed || []} scroll={{ x: 820 }} />
    {cancelTask ? <ConfirmDialog open title={t('cancelDownloadTitle')} detail={t('cancelDownloadDetail')} confirmLabel={t('cancelDownload')} cancelLabel={t('cancel')} pending={removeTask.isPending} onCancel={() => setCancelTask(null)} onConfirm={() => removeTask.mutate(cancelTask.id)} /> : null}
  </Card>
}

function DownloadProgress({ task }: { task: CoreDownloadTask }) {
  const percent = task.total_bytes > 0 ? Math.min(100, Math.round(task.downloaded_bytes * 100 / task.total_bytes)) : 0
  return <div className="min-w-48"><div className="mb-1 flex justify-between gap-3 text-xs text-[var(--muted)]"><span>{task.artifact || '—'}</span><span>{task.total_bytes > 0 ? `${formatBytes(task.downloaded_bytes)} / ${formatBytes(task.total_bytes)} · ${percent}%` : '—'}</span></div><div className="h-2 overflow-hidden rounded-full bg-[var(--surface-hover)]" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent} aria-label="download progress"><div className={`h-full rounded-full bg-emerald-500 transition-[width] ${task.total_bytes === 0 && task.state === 'running' ? 'w-1/3 animate-pulse' : ''}`} style={task.total_bytes > 0 ? { width: `${percent}%` } : undefined} /></div>{task.error ? <p className="mt-1 break-all text-xs text-red-600">{task.error}</p> : null}</div>
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`
  if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KiB`
  return `${(value / 1024 ** 2).toFixed(1)} MiB`
}

function taskTone(task: CoreDownloadTask): 'neutral' | 'success' | 'danger' | 'info' {
  if (task.state === 'failed') return 'danger'
  if (task.state === 'succeeded') return 'success'
  return task.stage === 'queued' ? 'neutral' : 'info'
}

function taskStage(task: CoreDownloadTask, t: ReturnType<typeof useI18n>['t']) {
  if (task.stage === 'resolving') return t('downloadResolving')
  if (task.stage === 'downloading') return t('downloading')
  if (task.stage === 'installing') return t('downloadInstalling')
  if (task.stage === 'completed') return t('downloadCompleted')
  if (task.stage === 'failed') return t('failed')
  return t('downloadQueued')
}

function isSelectedCore(cores: CoresResponse | undefined, item: CoreInstallation) {
  const selectedRepository = cores?.selected?.repository || ''
  const itemRepository = item.official ? '' : item.repository
  return cores?.selected?.core === item.core && selectedRepository === itemRepository && (cores.selected.reference === item.version || item.channels.includes(cores.selected.reference))
}

function refreshCoreQueries(queryClient: QueryClient) {
  queryClient.invalidateQueries({ queryKey: ['cores'] })
  queryClient.invalidateQueries({ queryKey: ['subscriptions'] })
  queryClient.invalidateQueries({ queryKey: ['system'] })
  queryClient.invalidateQueries({ queryKey: ['runtime', 'status'] })
}

function changeNotice(result: ChangeResult, queryClient: QueryClient, completed: string, deferred: string) {
  const needsRestart = Boolean(result.NeedsRestart || result.changes?.some((change) => change.NeedsRestart))
  const system = queryClient.getQueryData<SystemStatus>(['system'])
  const runtime = queryClient.getQueryData<ManagedRuntimeStatus>(['runtime', 'status'])
  return needsRestart && (system?.desired_state === 'stopped' || runtime?.desired_state === 'stopped') ? deferred : completed
}
