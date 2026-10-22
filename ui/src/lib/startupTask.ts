import type { RuntimePendingChange } from '../components/RestartChangeSummary'

export interface StartupLogEntry {
  sequence: number
  timestamp: string
  stage: string
  message: string
  change?: RuntimePendingChange
}

export interface StartupTask {
  id: string
  action: 'start' | 'restart' | 'stop'
  state: 'running' | 'succeeded' | 'failed'
  started_at: string
  finished_at: string | null
  logs: StartupLogEntry[]
  omitted_logs: number
  config_available: boolean
}

export function startupDuration(start: string, end: string | null, now: number) {
  const seconds = Math.max(0, Math.floor(((end ? Date.parse(end) : now) - Date.parse(start)) / 1000))
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
}

export function startupStageLabels(action: StartupTask['action']): Record<string, [string, string]> {
  const restart = action === 'restart'
  const stop = action === 'stop'
  return {
    begin: stop ? ['开始停止核心…', 'Starting core shutdown…'] : restart ? ['开始核心重启…', 'Starting core restart…'] : ['开始核心启动…', 'Starting core startup…'],
    recovery: ['恢复运行状态…', 'Recovering runtime state…'],
    runtime_lock: ['等待运行时锁…', 'Waiting for the runtime lock…'],
    preparing: ['准备运行配置…', 'Preparing runtime configuration…'],
    gateway: ['处理网关状态…', 'Updating gateway state…'],
    network_automation: ['处理自动网络配置…', 'Updating automatic network configuration…'],
    transparent_network: ['处理透明代理网络…', 'Updating transparent proxy networking…'],
    system_dns: ['处理系统 DNS…', 'Updating system DNS…'],
    dns_frontend: ['处理前置 DNS…', 'Updating DNS frontend…'],
    cn_routes: ['处理国内直连路由…', 'Updating domestic direct routes…'],
    cleanup: ['清理核心运行资源…', 'Cleaning up core runtime resources…'],
    timeout: ['操作超时', 'Operation timed out'],
    compiling: ['编译并校验新版配置…', 'Compiling and validating configuration…'],
    compiled: ['配置已准备就绪', 'Configuration ready'],
    stopping: ['停止当前核心…', 'Stopping current core…'],
    stopped: ['当前核心已停止', 'Current core stopped'],
    network: ['准备运行配置、前置 DNS 和网络环境…', 'Preparing runtime configuration, DNS frontend and networking…'],
    starting: ['启动核心…', 'Starting core…'],
    health_check: ['等待核心和网络健康检查…', 'Waiting for core and network health checks…'],
    healthy: ['核心和网络健康检查通过', 'Core and network health checks passed'],
    validating: ['校验运行配置…', 'Validating runtime configuration…'],
    succeeded: stop ? ['核心停止成功', 'Core shutdown succeeded'] : restart ? ['核心重启成功', 'Core restart succeeded'] : ['核心启动成功', 'Core startup succeeded'],
    failed: stop ? ['核心停止失败', 'Core shutdown failed'] : restart ? ['核心重启失败', 'Core restart failed'] : ['核心启动失败', 'Core startup failed'],
    error: ['错误', 'Error'],
  }
}
