import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'

const zh = {
  overview: '总览', proxies: '代理', connections: '连接', rules: '规则', traffic: '流量', logs: '日志', management: '管理',
  signIn: '登录', address: 'Sempre 地址', password: '管理员密码', connect: '连接', connecting: '正在连接',
  loginLead: '连接到 Sempre 控制面', addressHint: '默认使用当前页面地址，也可以连接其他 Sempre 实例。',
  emptyPassword: '当前管理员密码为空，建议立即设置。', logout: '退出登录', running: '运行中', stopped: '已停止', idle: '等待核心',
  service: '系统服务', core: '核心', version: '版本', mode: '模式', uptime: '启动时间', download: '下载', upload: '上传', memory: '内存',
  activeConnections: '活动连接', totalTraffic: '累计流量', realtimeTraffic: '实时流量', noCore: '尚未运行核心', noCoreDetail: '前往管理安装、选择核心并导入配置。',
  search: '搜索', refresh: '刷新', testLatency: '延迟测试', select: '选择', selected: '当前', provider: 'Provider', update: '更新', healthcheck: '健康检查',
  source: '来源', destination: '目标', process: '进程', chain: '代理链', rule: '规则', speed: '速率', close: '关闭', closeAll: '关闭全部',
  type: '类型', payload: '内容', outbound: '出站', noData: '暂无数据', noDataDetail: '核心尚未返回该功能的数据。',
  live: '实时', paused: '已暂停', pause: '暂停', resume: '继续', clear: '清空', export: '导出', level: '级别', message: '消息',
  coreTab: '核心', subscriptionTab: '订阅', configTab: '配置', webUITab: 'Web 与 UI',
  install: '安装', remove: '移除', use: '使用', reference: '核心引用', repository: '仓库', official: '官方', custom: '自定义', channel: '通道', installedVersions: '已安装版本',
  subscriptionURL: '订阅地址', schedule: '更新周期', save: '保存', updateNow: '立即更新', lastResult: '最近结果',
  commonSettings: '常用设置', jsonEditor: '完整 JSON', validate: '校验', validated: '配置校验通过', logLevel: '日志级别', routeFinal: '默认出站', dnsFinal: '默认 DNS', autoInterface: '自动检测网卡',
  listenAddress: '监听地址', passwordSet: '密码已设置', setPassword: '设置密码', clearPassword: '清空密码', uiSource: 'UI 来源', officialUI: '安装官方 UI', customURL: 'HTTPS ZIP 地址', uploadZIP: '上传 ZIP',
  apply: '应用', restart: '重启服务', stop: '停止服务', theme: '主题', language: '语言', systemTheme: '跟随系统', light: '浅色', dark: '深色',
  operationDone: '操作完成', operationFailed: '操作失败', loading: '加载中', details: '详情', host: '主机', device: '设备', user: '用户',
  lastHour: '最近一小时', historicalTraffic: '流量历史', currentRate: '当前速率', all: '全部', filter: '筛选',
} as const

type Key = keyof typeof zh
const en: Record<Key, string> = {
  overview: 'Overview', proxies: 'Proxies', connections: 'Connections', rules: 'Rules', traffic: 'Traffic', logs: 'Logs', management: 'Management',
  signIn: 'Sign in', address: 'Sempre address', password: 'Administrator password', connect: 'Connect', connecting: 'Connecting',
  loginLead: 'Connect to the Sempre control plane', addressHint: 'The current address is used by default. You can connect to another Sempre instance.',
  emptyPassword: 'The administrator password is empty. Set one as soon as possible.', logout: 'Sign out', running: 'Running', stopped: 'Stopped', idle: 'Waiting for core',
  service: 'System service', core: 'Core', version: 'Version', mode: 'Mode', uptime: 'Started', download: 'Download', upload: 'Upload', memory: 'Memory',
  activeConnections: 'Active connections', totalTraffic: 'Total traffic', realtimeTraffic: 'Realtime traffic', noCore: 'No core is running', noCoreDetail: 'Open Management to install and select a core, then import a configuration.',
  search: 'Search', refresh: 'Refresh', testLatency: 'Test latency', select: 'Select', selected: 'Selected', provider: 'Provider', update: 'Update', healthcheck: 'Health check',
  source: 'Source', destination: 'Destination', process: 'Process', chain: 'Chain', rule: 'Rule', speed: 'Speed', close: 'Close', closeAll: 'Close all',
  type: 'Type', payload: 'Payload', outbound: 'Outbound', noData: 'No data', noDataDetail: 'The core has not returned data for this capability.',
  live: 'Live', paused: 'Paused', pause: 'Pause', resume: 'Resume', clear: 'Clear', export: 'Export', level: 'Level', message: 'Message',
  coreTab: 'Core', subscriptionTab: 'Subscription', configTab: 'Configuration', webUITab: 'Web & UI',
  install: 'Install', remove: 'Remove', use: 'Use', reference: 'Core reference', repository: 'Repository', official: 'Official', custom: 'Custom', channel: 'Channel', installedVersions: 'Installed versions',
  subscriptionURL: 'Subscription URL', schedule: 'Update schedule', save: 'Save', updateNow: 'Update now', lastResult: 'Last result',
  commonSettings: 'Common settings', jsonEditor: 'Full JSON', validate: 'Validate', validated: 'Configuration is valid', logLevel: 'Log level', routeFinal: 'Default outbound', dnsFinal: 'Default DNS', autoInterface: 'Auto-detect interface',
  listenAddress: 'Listen address', passwordSet: 'Password set', setPassword: 'Set password', clearPassword: 'Clear password', uiSource: 'UI source', officialUI: 'Install official UI', customURL: 'HTTPS ZIP URL', uploadZIP: 'Upload ZIP',
  apply: 'Apply', restart: 'Restart service', stop: 'Stop service', theme: 'Theme', language: 'Language', systemTheme: 'System', light: 'Light', dark: 'Dark',
  operationDone: 'Operation completed', operationFailed: 'Operation failed', loading: 'Loading', details: 'Details', host: 'Host', device: 'Device', user: 'User',
  lastHour: 'Last hour', historicalTraffic: 'Traffic history', currentRate: 'Current rate', all: 'All', filter: 'Filter',
}

type Locale = 'zh-CN' | 'en'
const Context = createContext<{ locale: Locale; setLocale: (value: Locale) => void; t: (key: Key) => string } | null>(null)

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocale] = useState<Locale>(() => {
    const saved = localStorage.getItem('sempre.locale')
    if (saved === 'zh-CN' || saved === 'en') return saved
    return navigator.language.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en'
  })
  useEffect(() => {
    localStorage.setItem('sempre.locale', locale)
    document.documentElement.lang = locale
  }, [locale])
  const value = useMemo(() => ({ locale, setLocale, t: (key: Key) => (locale === 'zh-CN' ? zh[key] : en[key]) }), [locale])
  return <Context.Provider value={value}>{children}</Context.Provider>
}

export function useI18n() {
  const value = useContext(Context)
  if (!value) throw new Error('I18nProvider is missing')
  return value
}
