import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'

const zh = {
  overview: '总览', customNodes: '自定义节点', subscriptions: '订阅转换', proxies: '代理', connections: '连接', rules: '规则', traffic: '流量', logs: '日志', networkTest: '网络测试', gateway: 'LAN 网关', management: '管理',
  signIn: '登录', address: 'Sempre 地址', password: '管理员密码', connect: '连接', connecting: '正在连接',
  loginLead: '连接到 Sempre 控制面', addressHint: '默认使用当前页面地址，也可以连接其他 Sempre 实例。',
  emptyPassword: '当前管理员密码为空，建议立即设置。', logout: '退出登录', running: '运行中', stopped: '已停止', idle: '等待核心', starting: '正在启动', stopping: '正在停止', restarting: '正在重启', failed: '失败',
  service: '系统服务', core: '核心', version: '版本', mode: '模式', uptime: '启动时间', download: '下载', upload: '上传', memory: '内存',
  activeConnections: '活动连接', totalTraffic: '累计流量', realtimeTraffic: '实时流量', noCore: '尚未运行核心', noCoreDetail: '前往管理安装、选择核心并导入配置。',
  search: '搜索', refresh: '刷新', testLatency: '延迟测试', select: '选择', selected: '当前', provider: 'Provider', update: '更新', healthcheck: '健康检查',
  source: '来源', destination: '目标', process: '进程', chain: '代理链', rule: '规则', speed: '速率', close: '关闭', closeAll: '关闭全部',
  type: '类型', payload: '内容', outbound: '出站', noData: '暂无数据', noDataDetail: '核心尚未返回该功能的数据。', status: '状态',
  live: '实时', paused: '已暂停', pause: '暂停', resume: '继续', clear: '清空', export: '导出', level: '级别', message: '消息',
  runtimeTab: '运行状态', coreTab: '核心', subscriptionTab: '订阅', configTab: '配置', webUITab: 'Web 与 UI',
  install: '安装', remove: '移除', use: '使用', reference: '核心引用', repository: '仓库', official: '官方', custom: '自定义', channel: '通道', installedVersions: '已安装版本',
  subscriptionURL: '订阅地址', schedule: '更新周期', save: '保存', updateNow: '立即更新', lastResult: '最近结果',
  commonSettings: '常用设置', jsonEditor: '完整 JSON', validate: '校验', validated: '配置校验通过', logLevel: '日志级别', routeFinal: '默认出站', dnsFinal: '默认 DNS', autoInterface: '自动检测网卡',
  listenAddress: '监听地址', passwordSet: '密码已设置', setPassword: '设置密码', clearPassword: '清空密码', uiSource: 'UI 来源', officialUI: '安装官方 UI', customURL: 'HTTPS ZIP 地址', uploadZIP: '上传 ZIP', exportBundle: '导出部署包', exportBundleDetail: '导出的包包含订阅、节点、核心和当前 UI，但不包含管理员密码。',
  apply: '应用', restart: '重启服务', stop: '停止服务', theme: '主题', language: '语言', systemTheme: '跟随系统', light: '浅色', dark: '深色', expandSidebar: '展开侧栏', collapseSidebar: '收起侧栏',
  operationDone: '操作完成', operationFailed: '操作失败', changeDeferred: '变更已保存并通过校验，将在受管核心下次启动时生效。', loading: '加载中', details: '详情', host: '主机', device: '设备', user: '用户',
  networkTestDetail: '从 Sempre 主机并发访问国内外站点和出口 IP 接口。', networkTarget: '测试目标', domestic: '国内', foreign: '国外', reachable: '可达', unreachable: '不可达', latency: '延迟', averageLatency: '平均延迟', ipAddress: 'IP 地址', domesticIP: '国内出口 IP', foreignIP: '国外出口 IP', testingNetwork: '正在测试网络',
  lastHour: '最近一小时', historicalTraffic: '流量历史', currentRate: '当前速率', all: '全部', filter: '筛选',
  managedRuntime: '受管核心运行状态', sempreService: 'Sempre Service', managedCore: '受管核心', online: '在线', desiredState: '期望状态', actualState: '实际状态', selectedReference: '选择引用', configuration: '活动配置', runtimeUptime: '运行时长', restarts: '自动重启', lastTransition: '最近变化', lastExit: '最近退出', lastError: '最近错误', coreNotRunning: '受管核心未运行', coreNotRunningDetail: '核心运行后将显示流量和连接数据。',
	startCore: '启动受管核心', stopCore: '停止受管核心', restartCore: '重启受管核心', operationAccepted: '操作已受理', pendingChange: '存在待应用的核心或配置变更，将在核心健康运行后提交。', pendingHealthCheck: '新核心或配置正在健康验证，约 10 秒后自动提交。', viewLogs: '查看日志', coreStopTitle: '停止受管核心？', coreStopWarning: '停止 {core} 会立即中断当前代理流量。Sempre Service、Web 控制台和 API 将继续运行。', cancel: '取消', confirm: '确认',
  systemServiceActions: 'Sempre 系统服务', dangerZone: '低频危险操作', serviceRestartWarning: '重启 Sempre Service 会暂时中断 Web 控制台和 API，受管核心将按期望状态恢复。', serviceStopTitle: '停止 Sempre Service？', serviceStopWarning: '停止后当前页面、API 和自动管理都会失联；Web 页面无法自行重新启动服务。', serviceStopAcknowledgement: '我已了解：停止后必须在主机执行 sempre service start，或通过操作系统服务管理器重新启动 Sempre。',
  defaultSubscription: '默认订阅', addProfile: '新增订阅', profileName: '订阅名称', sources: '订阅源', addURL: '添加 URL', addRaw: '添加原始内容', rawContent: '原始订阅内容', prefix: '节点前缀', userAgent: 'User-Agent', fetchMode: '抓取模式', enabled: '启用', test: '测试', nodeLibrary: '节点库', groupsAndRules: '分组与规则', dnsAndPrivate: 'DNS 与私网访问', diagnostics: '诊断', preview: '预览', compilerTarget: '编译目标', automaticRestart: '定时更新后自动重启', restartNow: '立即重启核心', activate: '激活', activeProfile: '当前订阅', filters: '节点过滤器', groups: '代理分组', ruleProviders: '规则集', customRules: '自定义规则', dnsConfig: 'DNS 配置', privateAccess: '私网访问', customConfig: '高级配置', targetOverrides: '目标配置覆盖', systemGroups: '使用系统分组', systemRuleProviders: '使用系统规则集', systemFilters: '使用系统过滤词', systemCustomRules: '使用系统自定义规则', systemDNS: '使用系统 DNS', addNode: '新增节点', editNode: '编辑节点', nodeJSON: 'Clash 节点 JSON', saveAndStage: '保存、校验并暂存', clearCache: '清理抓取缓存', droppedFields: '未映射字段', traceNode: '追踪节点字段', noSources: '尚未配置订阅源', staged: '配置已暂存，重启核心后生效。',
  subscriptionSets: '订阅集', defaultSubscriptionSet: '默认订阅集', newSubscriptionSet: '新建订阅集', createSubscriptionSet: '创建', manageSubscriptionSet: '管理订阅集', renameSubscriptionSet: '重命名订阅集', deleteSubscriptionSet: '删除订阅集', subscriptionSetName: '订阅集名称', subscriptionSetNameRequired: '请输入订阅集名称。', subscriptionSetNameUsed: '该订阅集名称已被使用。', activeSubscriptionSet: '当前订阅集', activateSubscriptionSet: '激活订阅集', alreadyActiveSubscriptionSet: '已经是当前订阅集', activeSubscriptionSetDeleteReason: '当前订阅集不能删除', lastSubscriptionSetDeleteReason: '至少保留一个订阅集', deleteSubscriptionSetDetail: '将永久删除订阅集：',
} as const

type Key = keyof typeof zh
const en: Record<Key, string> = {
  overview: 'Overview', customNodes: 'Custom Nodes', subscriptions: 'Subscriptions', proxies: 'Proxies', connections: 'Connections', rules: 'Rules', traffic: 'Traffic', logs: 'Logs', networkTest: 'Network Test', gateway: 'LAN Gateway', management: 'Management',
  signIn: 'Sign in', address: 'Sempre address', password: 'Administrator password', connect: 'Connect', connecting: 'Connecting',
  loginLead: 'Connect to the Sempre control plane', addressHint: 'The current address is used by default. You can connect to another Sempre instance.',
  emptyPassword: 'The administrator password is empty. Set one as soon as possible.', logout: 'Sign out', running: 'Running', stopped: 'Stopped', idle: 'Waiting for core', starting: 'Starting', stopping: 'Stopping', restarting: 'Restarting', failed: 'Failed',
  service: 'System service', core: 'Core', version: 'Version', mode: 'Mode', uptime: 'Started', download: 'Download', upload: 'Upload', memory: 'Memory',
  activeConnections: 'Active connections', totalTraffic: 'Total traffic', realtimeTraffic: 'Realtime traffic', noCore: 'No core is running', noCoreDetail: 'Open Management to install and select a core, then import a configuration.',
  search: 'Search', refresh: 'Refresh', testLatency: 'Test latency', select: 'Select', selected: 'Selected', provider: 'Provider', update: 'Update', healthcheck: 'Health check',
  source: 'Source', destination: 'Destination', process: 'Process', chain: 'Chain', rule: 'Rule', speed: 'Speed', close: 'Close', closeAll: 'Close all',
  type: 'Type', payload: 'Payload', outbound: 'Outbound', noData: 'No data', noDataDetail: 'The core has not returned data for this capability.', status: 'Status',
  live: 'Live', paused: 'Paused', pause: 'Pause', resume: 'Resume', clear: 'Clear', export: 'Export', level: 'Level', message: 'Message',
  runtimeTab: 'Runtime', coreTab: 'Core', subscriptionTab: 'Subscription', configTab: 'Configuration', webUITab: 'Web & UI',
  install: 'Install', remove: 'Remove', use: 'Use', reference: 'Core reference', repository: 'Repository', official: 'Official', custom: 'Custom', channel: 'Channel', installedVersions: 'Installed versions',
  subscriptionURL: 'Subscription URL', schedule: 'Update schedule', save: 'Save', updateNow: 'Update now', lastResult: 'Last result',
  commonSettings: 'Common settings', jsonEditor: 'Full JSON', validate: 'Validate', validated: 'Configuration is valid', logLevel: 'Log level', routeFinal: 'Default outbound', dnsFinal: 'Default DNS', autoInterface: 'Auto-detect interface',
  listenAddress: 'Listen address', passwordSet: 'Password set', setPassword: 'Set password', clearPassword: 'Clear password', uiSource: 'UI source', officialUI: 'Install official UI', customURL: 'HTTPS ZIP URL', uploadZIP: 'Upload ZIP', exportBundle: 'Export deployment bundle', exportBundleDetail: 'The bundle includes subscriptions, nodes, cores, and the current UI, but not the administrator password.',
  apply: 'Apply', restart: 'Restart service', stop: 'Stop service', theme: 'Theme', language: 'Language', systemTheme: 'System', light: 'Light', dark: 'Dark', expandSidebar: 'Expand sidebar', collapseSidebar: 'Collapse sidebar',
  operationDone: 'Operation completed', operationFailed: 'Operation failed', changeDeferred: 'The validated change was saved and will take effect the next time the managed core starts.', loading: 'Loading', details: 'Details', host: 'Host', device: 'Device', user: 'User',
  networkTestDetail: 'Run concurrent site and public IP checks from the Sempre host.', networkTarget: 'Target', domestic: 'Domestic', foreign: 'Foreign', reachable: 'Reachable', unreachable: 'Unreachable', latency: 'Latency', averageLatency: 'Average latency', ipAddress: 'IP address', domesticIP: 'Domestic IP', foreignIP: 'Foreign IP', testingNetwork: 'Testing network',
  lastHour: 'Last hour', historicalTraffic: 'Traffic history', currentRate: 'Current rate', all: 'All', filter: 'Filter',
  managedRuntime: 'Managed core runtime', sempreService: 'Sempre Service', managedCore: 'Managed core', online: 'Online', desiredState: 'Desired state', actualState: 'Actual state', selectedReference: 'Selected reference', configuration: 'Active configuration', runtimeUptime: 'Uptime', restarts: 'Automatic restarts', lastTransition: 'Last transition', lastExit: 'Last exit', lastError: 'Last error', coreNotRunning: 'Managed core is not running', coreNotRunningDetail: 'Traffic and connection data will appear after the core is running.',
	startCore: 'Start managed core', stopCore: 'Stop managed core', restartCore: 'Restart managed core', operationAccepted: 'Operation accepted', pendingChange: 'A core or configuration change is pending and will be committed after the core runs successfully.', pendingHealthCheck: 'The new core or configuration is being health-checked and will be committed after about 10 seconds.', viewLogs: 'View logs', coreStopTitle: 'Stop the managed core?', coreStopWarning: 'Stopping {core} immediately interrupts current proxy traffic. Sempre Service, the Web console, and the API will remain online.', cancel: 'Cancel', confirm: 'Confirm',
  systemServiceActions: 'Sempre system service', dangerZone: 'Infrequent dangerous actions', serviceRestartWarning: 'Restarting Sempre Service temporarily disconnects the Web console and API. The managed core will return to its desired state.', serviceStopTitle: 'Stop Sempre Service?', serviceStopWarning: 'This page, the API, and automatic management become unavailable after stopping. The Web page cannot restart the service itself.', serviceStopAcknowledgement: 'I understand that I must run sempre service start on the host or use the operating system service manager to start Sempre again.',
  defaultSubscription: 'Default', addProfile: 'Add subscription', profileName: 'Subscription name', sources: 'Sources', addURL: 'Add URL', addRaw: 'Add raw content', rawContent: 'Raw subscription content', prefix: 'Node prefix', userAgent: 'User-Agent', fetchMode: 'Fetch mode', enabled: 'Enabled', test: 'Test', nodeLibrary: 'Node library', groupsAndRules: 'Groups & rules', dnsAndPrivate: 'DNS & private access', diagnostics: 'Diagnostics', preview: 'Preview', compilerTarget: 'Compiler target', automaticRestart: 'Restart after scheduled updates', restartNow: 'Restart core now', activate: 'Activate', activeProfile: 'Active profile', filters: 'Node filters', groups: 'Proxy groups', ruleProviders: 'Rule providers', customRules: 'Custom rules', dnsConfig: 'DNS configuration', privateAccess: 'Private access', customConfig: 'Advanced configuration', targetOverrides: 'Target configuration overrides', systemGroups: 'Use system groups', systemRuleProviders: 'Use system rule providers', systemFilters: 'Use system filters', systemCustomRules: 'Use system custom rules', systemDNS: 'Use system DNS', addNode: 'Add node', editNode: 'Edit node', nodeJSON: 'Clash node JSON', saveAndStage: 'Save, validate & stage', clearCache: 'Clear fetch cache', droppedFields: 'Unmapped fields', traceNode: 'Trace node fields', noSources: 'No subscription sources', staged: 'Configuration staged. Restart the core to apply it.',
  subscriptionSets: 'Subscription sets', defaultSubscriptionSet: 'Default subscription set', newSubscriptionSet: 'New subscription set', createSubscriptionSet: 'Create', manageSubscriptionSet: 'Manage subscription set', renameSubscriptionSet: 'Rename subscription set', deleteSubscriptionSet: 'Delete subscription set', subscriptionSetName: 'Subscription set name', subscriptionSetNameRequired: 'Enter a subscription set name.', subscriptionSetNameUsed: 'That subscription set name is already in use.', activeSubscriptionSet: 'Active subscription set', activateSubscriptionSet: 'Activate subscription set', alreadyActiveSubscriptionSet: 'Already the active subscription set', activeSubscriptionSetDeleteReason: 'The active subscription set cannot be deleted', lastSubscriptionSetDeleteReason: 'At least one subscription set is required', deleteSubscriptionSetDetail: 'Permanently delete subscription set:',
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
