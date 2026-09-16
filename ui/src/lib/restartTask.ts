import type { RuntimePendingChange } from '../components/RestartChangeSummary'

export interface RestartLogEntry {
  sequence: number
  timestamp: string
  stage: string
  message: string
  change?: RuntimePendingChange
}

export interface RestartTask {
  id: string
  state: 'running' | 'succeeded' | 'failed'
  started_at: string
  finished_at: string | null
  logs: RestartLogEntry[]
  omitted_logs: number
  config_available: boolean
}

export function restartDuration(start: string, end: string | null, now: number) {
  const seconds = Math.max(0, Math.floor(((end ? Date.parse(end) : now) - Date.parse(start)) / 1000))
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
}

export const restartStageLabels: Record<string, [string, string]> = {
  begin: ['开始核心重启…', 'Starting core restart…'],
  compiling: ['编译并校验新版配置…', 'Compiling and validating configuration…'],
  compiled: ['配置编译、校验成功', 'Configuration compiled and validated'],
  stopping: ['停止当前核心…', 'Stopping current core…'],
  stopped: ['当前核心已停止', 'Current core stopped'],
  network: ['准备运行配置、前置 DNS 和网络环境…', 'Preparing runtime configuration, DNS frontend and networking…'],
  starting: ['启动核心…', 'Starting core…'],
  health_check: ['等待核心和网络健康检查…', 'Waiting for core and network health checks…'],
  healthy: ['核心健康检查通过', 'Core health checks passed'],
  succeeded: ['核心重启成功', 'Core restart succeeded'],
  failed: ['核心重启失败', 'Core restart failed'],
  error: ['错误', 'Error'],
}
