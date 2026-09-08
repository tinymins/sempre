import { useEffect, useState } from 'react'
import { Alert, Button, Modal, Progress } from '@acme/components'
import { Check, CheckCircle2, CircleAlert, Download, LoaderCircle, PackageCheck, RefreshCw } from 'lucide-react'
import { useI18n } from '../../lib/i18n'
import type { ServiceUpdateTask } from '../../lib/types'

const stages = ['prepare', 'download', 'verify', 'install', 'complete'] as const

export function ServiceUpdateModal({ open, task, targetVersion, submitting, disconnected, error, allowClose = true, onClose, onRelogin }: {
  open: boolean
  task?: ServiceUpdateTask | null
  targetVersion: string
  submitting: boolean
  disconnected: boolean
  error?: string
  allowClose?: boolean
  onClose: () => void
  onRelogin: () => void
}) {
  const { locale } = useI18n()
  const zh = locale === 'zh-CN'
  const [now, setNow] = useState(Date.now)
  const running = submitting || task?.state === 'running'
  const succeeded = task?.state === 'succeeded'
  const failed = task?.state === 'failed' || Boolean(error && !running)
  const stage = disconnected && running ? 'installing' : task?.stage || 'checking'
  const activeStep = stageIndex(stage, succeeded)
  const elapsed = duration(task?.started_at, task?.finished_at, now)
  const title = succeeded ? (zh ? 'Sempre 更新完成' : 'Sempre update complete')
    : failed ? (zh ? 'Sempre 更新失败' : 'Sempre update failed')
      : (zh ? '正在更新 Sempre' : 'Updating Sempre')

  useEffect(() => {
    if (!open || !running) return
    const timer = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [open, running])

  return <Modal open={open} centered width="min(760px, calc(100vw - 32px))" maskClosable={false} closable={!succeeded && allowClose} keyboard={!succeeded && allowClose} onCancel={onClose}
    title={<span className="flex items-center gap-2.5">{succeeded ? <CheckCircle2 size={20} className="text-emerald-500" /> : failed ? <CircleAlert size={20} className="text-red-500" /> : <LoaderCircle aria-label="loading" size={20} className="animate-spin text-cyan-500" />}<span>{title}</span><span className="font-mono text-sm tabular-nums text-[var(--muted)]">({elapsed})</span></span>}
    footer={<div className="flex w-full items-center justify-between gap-4"><span className="text-xs text-[var(--muted)]">{succeeded ? (zh ? '更新已完成，请重新登录以使用新版本。' : 'The update is complete. Sign in again to use the new version.') : running ? (allowClose ? (zh ? '任务在后台执行，关闭窗口不会中止更新。' : 'The update continues in the background if this window is closed.') : (zh ? '正在等待更新完成，请稍候。' : 'Waiting for the update to finish.')) : (zh ? '当前安装保持原状，可以关闭后重试。' : 'The current installation is unchanged. Close this window to retry.')}</span>{succeeded ? <Button variant="primary" onClick={onRelogin}>{zh ? '重新登录' : 'Sign in again'}</Button> : allowClose ? <Button onClick={onClose}>{zh ? '关闭' : 'Close'}</Button> : null}</div>}>
    <div className="space-y-5">
      <div className="rounded-lg bg-[var(--surface-hover)] px-4 py-3">
        <div className="flex items-center justify-between gap-4"><Version label={zh ? '当前版本' : 'Current'} value={task?.current_version || '—'} /><span className="text-[var(--muted)]">→</span><Version align="right" label={zh ? '目标版本' : 'Target'} value={task?.target_version || targetVersion || (zh ? '查询中' : 'Checking')} /></div>
      </div>
      <ol aria-label={zh ? '更新步骤' : 'Update steps'} className="grid grid-cols-5 gap-1">
        {stages.map((item, index) => <li key={item} className="min-w-0 text-center"><div className="flex items-center"><span className={`h-px flex-1 ${index === 0 ? 'bg-transparent' : index <= activeStep ? 'bg-emerald-500' : 'bg-[var(--border)]'}`} /><span className={`grid size-7 shrink-0 place-items-center rounded-full border text-xs ${index < activeStep || succeeded ? 'border-emerald-500 bg-emerald-500 text-white' : index === activeStep ? 'border-cyan-500 bg-cyan-500/10 text-cyan-500' : 'border-[var(--border)] text-[var(--muted)]'}`}>{index < activeStep || succeeded ? <Check size={14} /> : index + 1}</span><span className={`h-px flex-1 ${index === stages.length - 1 ? 'bg-transparent' : index < activeStep ? 'bg-emerald-500' : 'bg-[var(--border)]'}`} /></div><span className={`mt-1 block truncate text-[11px] ${index === activeStep && !succeeded ? 'font-medium text-[var(--text)]' : 'text-[var(--muted)]'}`}>{stepLabel(item, zh)}</span></li>)}
      </ol>
      {failed ? <div role="alert"><Alert type="error" showIcon message={task?.error || error || (zh ? '更新任务失败。' : 'The update failed.')} /></div> : <CurrentStage task={task} stage={stage} submitting={submitting} disconnected={disconnected} zh={zh} />}
    </div>
  </Modal>
}

function CurrentStage({ task, stage, submitting, disconnected, zh }: { task?: ServiceUpdateTask | null; stage: string; submitting: boolean; disconnected: boolean; zh: boolean }) {
  const downloading = stage === 'downloading'
  const percent = task?.total_bytes ? Math.min(100, task.downloaded_bytes * 100 / task.total_bytes) : 0
  const icon = downloading ? <Download size={24} /> : stage === 'completed' ? <PackageCheck size={24} /> : disconnected ? <RefreshCw size={24} className="animate-spin" /> : <LoaderCircle size={24} className="animate-spin" />
  const [title, detail] = stageCopy(stage, submitting, disconnected, zh)
  return <div role="status" className="rounded-lg border border-[var(--border)] p-5">
    <div className="flex items-start gap-3"><span className="mt-0.5 text-cyan-500">{icon}</span><div className="min-w-0 flex-1"><p className="font-semibold">{title}</p><p className="mt-1 text-sm text-[var(--muted)]">{detail}</p></div></div>
    {downloading && task ? <div className="mt-5 space-y-2"><div role="progressbar" aria-label={zh ? '更新下载进度' : 'Update download progress'} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percent)}><Progress percent={percent} status="active" /></div><div className="grid grid-cols-2 gap-3 text-xs text-[var(--muted)] sm:grid-cols-3"><Metric label={zh ? '下载进度' : 'Downloaded'} value={`${formatBytes(task.downloaded_bytes)} / ${formatBytes(task.total_bytes)}`} /><Metric label={zh ? '实时速度' : 'Speed'} value={task.bytes_per_second ? `${formatBytes(task.bytes_per_second)}/s` : '—'} /><Metric label={zh ? '预计剩余' : 'ETA'} value={task.eta_seconds == null ? '—' : formatEta(task.eta_seconds, zh)} /></div>{task.artifact ? <p className="truncate font-mono text-[11px] text-[var(--muted)]">{task.artifact}</p> : null}</div> : null}
  </div>
}

function Version({ label, value, align = 'left' }: { label: string; value: string; align?: 'left' | 'right' }) {
  const display = /^v?\d/.test(value) ? `v${value.replace(/^v/, '')}` : value
  return <div className={align === 'right' ? 'text-right' : ''}><p className="text-xs text-[var(--muted)]">{label}</p><p className="mt-1 font-mono text-sm font-semibold">{display}</p></div>
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div><p>{label}</p><p className="mt-0.5 font-mono font-medium text-[var(--text)]">{value}</p></div>
}

function stageIndex(stage: string, succeeded: boolean) {
  if (succeeded || stage === 'completed') return 4
  if (stage === 'installing') return 3
  if (['verifying', 'extracting', 'validating'].includes(stage)) return 2
  if (stage === 'downloading') return 1
  return 0
}

function stepLabel(step: typeof stages[number], zh: boolean) {
  const labels = { prepare: ['准备', 'Prepare'], download: ['下载', 'Download'], verify: ['校验', 'Verify'], install: ['安装', 'Install'], complete: ['完成', 'Complete'] }
  return labels[step][zh ? 0 : 1]
}

function stageCopy(stage: string, submitting: boolean, disconnected: boolean, zh: boolean) {
  if (submitting) return zh ? ['正在创建更新任务', '服务正在准备后台更新任务。'] : ['Creating update task', 'The service is preparing the background update task.']
  if (disconnected) return zh ? ['正在安装并重启服务', '服务暂时离线，正在等待新版本恢复连接。'] : ['Installing and restarting', 'The service is temporarily offline while the new version starts.']
  const copy: Record<string, [string, string]> = zh ? {
    checking: ['正在检查更新', '正在从 sempre.run 获取并验证发布清单。'], resolving: ['正在准备下载', '正在选择适合当前系统和架构的发布资源。'], downloading: ['正在下载安装包', '下载完成后将自动校验文件完整性。'], verifying: ['正在校验 SHA-256', '正在确认安装包未损坏且与发布摘要一致。'], extracting: ['正在解压安装包', '正在将经过校验的发布包解压到临时目录。'], validating: ['正在验证发布包', '正在检查 Bundle 完整性和可执行文件版本。'], installing: ['正在安装并重启服务', '即将短暂断开连接并原子替换当前安装。'], completed: ['更新完成', '新版本服务已恢复并通过版本检查。'],
  } : {
    checking: ['Checking for updates', 'Fetching and validating the release manifest from sempre.run.'], resolving: ['Preparing download', 'Selecting the release asset for this operating system and architecture.'], downloading: ['Downloading update', 'The package will be verified automatically after download.'], verifying: ['Verifying SHA-256', 'Confirming that the package matches the published digest.'], extracting: ['Extracting package', 'Extracting the verified release into a temporary directory.'], validating: ['Validating release', 'Checking bundle integrity and the executable version.'], installing: ['Installing and restarting', 'The connection will briefly close while the installation is replaced atomically.'], completed: ['Update complete', 'The new service is online and reported the expected version.'],
  }
  return copy[stage] || copy.checking
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`
  if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KiB`
  return `${(value / 1024 ** 2).toFixed(1)} MiB`
}

function formatEta(seconds: number, zh: boolean) {
  if (seconds < 60) return zh ? `约 ${seconds} 秒` : `about ${seconds}s`
  const minutes = Math.ceil(seconds / 60)
  return zh ? `约 ${minutes} 分钟` : `about ${minutes}m`
}

function duration(start: string | undefined, finish: string | undefined, now: number) {
  if (!start) return '00:00'
  const seconds = Math.max(0, Math.floor(((finish ? Date.parse(finish) : now) - Date.parse(start)) / 1000))
  return `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`
}
