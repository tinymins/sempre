import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Alert, Button, Modal } from '@acme/components'
import { CheckCircle2, CircleAlert, LoaderCircle } from 'lucide-react'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { restartDuration, restartStageLabels, type RestartLogEntry, type RestartTask } from '../lib/restartTask'
import { formatPendingChange } from './RestartChangeSummary'

export function RuntimeRestartModal({ open, task, submittedAt, submitting, error, onClose }: {
  open: boolean
  task?: RestartTask | null
  submittedAt: string
  submitting: boolean
  error?: string
  onClose: () => void
}) {
  const { locale, t } = useI18n()
  const zh = locale === 'zh-CN'
  const running = submitting || task?.state === 'running'
  const [now, setNow] = useState(Date.now)
  const [configOpen, setConfigOpen] = useState(false)
  const logRef = useRef<HTMLDivElement>(null)
  const attachLog = useCallback((node: HTMLDivElement | null) => {
    logRef.current = node
    if (node) node.scrollTop = node.scrollHeight
  }, [])
  const current = submitting ? null : task
  const title = running ? (zh ? '正在重启核心' : 'Restarting core')
    : current?.state === 'succeeded' ? (zh ? '核心重启成功' : 'Core restart succeeded')
      : current ? (zh ? '核心重启失败' : 'Core restart failed') : (zh ? '无法确认重启状态' : 'Restart status unavailable')
  const elapsed = restartDuration(current?.started_at || submittedAt, current?.finished_at || null, now)
  const lastSequence = current?.logs.at(-1)?.sequence

  useEffect(() => {
    if (!open || !running) return
    const timer = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [open, running])
  useEffect(() => {
    if (open && logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight
  }, [open, lastSequence])

  return <>
    <Modal open={open} centered width="min(1000px, calc(100vw - 32px))" maskClosable={false} onCancel={onClose}
      title={<span className="flex items-center gap-2.5">{running ? <LoaderCircle aria-label="loading" size={20} className="animate-spin text-cyan-500" /> : current?.state === 'succeeded' ? <CheckCircle2 size={20} className="text-emerald-500" /> : <CircleAlert size={20} className="text-red-500" />}<span>{title}</span><span className="font-mono text-sm tabular-nums text-[var(--muted)]">({elapsed})</span></span>}
      footer={<div className="flex w-full items-center justify-between gap-4"><span className="text-xs text-[var(--muted)]">{running ? (zh ? '任务在后台执行，关闭窗口不会取消重启。' : 'Runs in the background. Closing this window does not cancel the restart.') : (zh ? '任务已结束，日志保留至下一次重启或服务退出。' : 'Logs remain until the next restart or service shutdown.')}</span><Button onClick={onClose}>{t('close')}</Button></div>}>
      <div className="space-y-3">
        <div role="status" className="flex flex-wrap items-center justify-between gap-2 text-xs text-[var(--muted)]">
          <span>{zh ? '开始时间：' : 'Started: '}{new Date(current?.started_at || submittedAt).toLocaleString(locale)}</span>
          <span>{running ? (zh ? '执行中 · 实时日志' : 'Running · live output') : current?.state === 'rolled_back' ? (zh ? '失败 · 已回滚' : 'Failed · rolled back') : title}</span>
        </div>
        {error ? <div role="alert"><Alert type="error" showIcon message={error} /></div> : null}
        <div ref={attachLog} role="log" aria-label={zh ? '核心重启日志' : 'Core restart log'} aria-live="polite" tabIndex={0} onKeyDown={selectAllContents}
          className="h-[min(56vh,560px)] min-h-48 overflow-auto rounded-md border border-slate-700 bg-slate-950 p-4 font-mono text-xs leading-6 text-slate-200 [color-scheme:dark]">
          {current?.omitted_logs ? <p className="text-amber-300">{zh ? `已省略前 ${current.omitted_logs} 条日志` : `${current.omitted_logs} earlier entries omitted`}</p> : null}
          {current?.logs.map((entry) => <LogLine key={entry.sequence} entry={entry} configAvailable={current.config_available} onConfig={() => setConfigOpen(true)} />)}
          {!current ? <p><span className="text-slate-500">[{new Date(submittedAt).toLocaleTimeString(locale, { hour12: false })}] </span>{submitting ? (zh ? '正在提交重启任务…' : 'Submitting restart task…') : (zh ? '未能确认任务状态，请检查连接；可稍后重新打开任务日志。' : 'Could not confirm task status. Check the connection and reopen the task log.')}</p> : null}
        </div>
      </div>
    </Modal>
    {configOpen && current ? <RestartConfigModal task={current} onClose={() => setConfigOpen(false)} /> : null}
  </>
}

function LogLine({ entry, configAvailable, onConfig }: { entry: RestartLogEntry; configAvailable: boolean; onConfig: () => void }) {
  const { locale, t } = useI18n()
  const zh = locale === 'zh-CN'
  const label = entry.change ? formatPendingChange(entry.change, t, locale) : restartStageLabels[entry.stage]?.[zh ? 0 : 1]
  const raw = ['stdout', 'stderr', 'validation', 'supervisor'].includes(entry.stage)
  const text = [label || (raw ? `[${entry.stage}]` : entry.stage), entry.message].filter(Boolean).join(' ')
  return <div className={`whitespace-pre-wrap break-words ${['error', 'failed', 'rolled_back'].includes(entry.stage) ? 'text-red-300' : entry.stage === 'succeeded' ? 'text-emerald-300' : raw ? 'text-slate-400' : ''}`}>
    {text.split('\n').map((line, index) => <div key={index}><span className="select-none text-slate-500">[{new Date(entry.timestamp).toLocaleTimeString(locale, { hour12: false })}] </span>{line}
      {entry.stage === 'compiled' && configAvailable && index === 0 ? <Button variant="link" size="small" className="ml-2 !text-cyan-300" onClick={onConfig}>{zh ? '[查看完整配置]' : '[View full configuration]'}</Button> : null}
    </div>)}
  </div>
}

function RestartConfigModal({ task, onClose }: { task: RestartTask; onClose: () => void }) {
  const { session } = useSession()
  const { locale } = useI18n()
  const config = useQuery({
    queryKey: ['runtime', 'restart-config', task.id],
    queryFn: () => api<{ hash: string; content: string }>(session!, `/runtime/restart/config?id=${encodeURIComponent(task.id)}`),
    staleTime: Infinity,
  })
  return <Modal open centered width="min(1100px, calc(100vw - 32px))" zIndex={1100} title={locale === 'zh-CN' ? '本次重启的完整配置（含敏感信息）' : 'Configuration for this restart (contains sensitive values)'} footer={null} onCancel={onClose}>
    {config.isPending ? <LoaderCircle className="animate-spin" /> : config.error ? <Alert type="error" message={config.error.message} /> : <pre aria-label={locale === 'zh-CN' ? '完整配置' : 'Full configuration'} tabIndex={0} onKeyDown={selectAllContents} className="max-h-[65vh] overflow-auto rounded-md bg-slate-950 p-4 text-xs leading-5 text-slate-200">{config.data.content}</pre>}
  </Modal>
}

function selectAllContents(event: KeyboardEvent<HTMLElement>) {
  if (!(event.ctrlKey || event.metaKey) || event.altKey || event.key.toLowerCase() !== 'a') return
  event.preventDefault()
  const selection = window.getSelection()
  if (!selection) return
  const range = document.createRange()
  range.selectNodeContents(event.currentTarget)
  selection.removeAllRanges()
  selection.addRange(range)
}
