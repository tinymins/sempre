import { useI18n } from '../../lib/i18n'

const zh = {
  serverSubtitle: '多用户订阅转换与配置管理', email: '邮箱', password: '密码', passwordHint: '至少使用 12 个字符。', signIn: '登录', createAccount: '创建账号', createAccountLink: '创建服务器账号', alreadyAccount: '已有账号？登录',
  signOut: '退出登录', createProfile: '创建配置', profileName: '配置名称', create: '创建', revision: '版本', updated: '更新于', multiUserProfile: '多人配置', noProfiles: '暂无配置。',
  back: '返回配置列表', save: '保存', savedRevision: '已保存版本 {revision}。', published: '已发布 {nodes} 个节点 · {hash}', shareCreated: '分享链接已创建，请立即复制；令牌之后无法恢复。', memberUpdated: '成员权限已更新。', readOnly: '此共享配置为只读。',
  outputTarget: '输出目标', refreshNow: '立即刷新并发布', previewResult: '预览结果', createShare: '创建分享链接', autoRefresh: '自动刷新并发布此目标', lastRefresh: '最近刷新', nextRefresh: '下次', shareStats: '{shares} 条分享记录 · 累计下载 {total} 次，今日 {today} 次。后续草稿失败时，公开链接会继续提供最后一次成功产物。', compiledTitle: '已编译产物与诊断', represented: '已转换 {nodes} 个节点 · 省略 {omitted} 个',
  members: '成员', registeredEmail: '已注册用户邮箱', role: '角色', viewer: '只读', editor: '编辑', addOrUpdate: '添加或更新',
  diagnosticsTitle: '订阅源与转换诊断', diagnosticsDetail: '实时测试已保存订阅源、检查合并节点列表，并追踪节点的过滤与目标转换过程。', source: '订阅源', testSource: '立即测试订阅源', clearCache: '清理订阅源缓存', cacheCleared: '订阅源缓存已清理。', previewNodes: '预览合并节点', node: '节点', traceNode: '追踪所选节点', protocol: '协议', endpoint: '端点', sourceIndex: '来源',
  intervalHint: '周期格式示例：30m、12h 或 1d。',
} as const

type Key = keyof typeof zh
const en: Record<Key, string> = {
  serverSubtitle: 'Multi-user subscription conversion and profile management', email: 'Email', password: 'Password', passwordHint: 'Use at least 12 characters.', signIn: 'Sign in', createAccount: 'Create account', createAccountLink: 'Create a server account', alreadyAccount: 'Already have an account? Sign in',
  signOut: 'Sign out', createProfile: 'Create profile', profileName: 'Profile name', create: 'Create', revision: 'Revision', updated: 'Updated', multiUserProfile: 'Multi-user profile', noProfiles: 'No profiles yet.',
  back: 'Back to profiles', save: 'Save', savedRevision: 'Saved revision {revision}.', published: 'Published {nodes} nodes · {hash}', shareCreated: 'Share link created. Copy it now; the token cannot be recovered later.', memberUpdated: 'Member access updated.', readOnly: 'This shared profile is read-only.',
  outputTarget: 'Output target', refreshNow: 'Refresh and publish now', previewResult: 'Preview result', createShare: 'Create share link', autoRefresh: 'Automatically refresh and publish this target', lastRefresh: 'Last refresh', nextRefresh: 'next', shareStats: '{shares} share record(s) · {total} artifact download(s), {today} today. The public link keeps serving the last successful artifact if a later draft fails.', compiledTitle: 'Compiled artifact and diagnostics', represented: '{nodes} represented node(s) · {omitted} omitted',
  members: 'Members', registeredEmail: 'Registered user email', role: 'Role', viewer: 'Viewer', editor: 'Editor', addOrUpdate: 'Add or update',
  diagnosticsTitle: 'Source and conversion diagnostics', diagnosticsDetail: 'Test a saved source live, inspect the merged node list, and trace one node through filtering and target conversion.', source: 'Source', testSource: 'Test source now', clearCache: 'Clear source cache', cacheCleared: 'Source cache cleared.', previewNodes: 'Preview merged nodes', node: 'Node', traceNode: 'Trace selected node', protocol: 'Protocol', endpoint: 'Endpoint', sourceIndex: 'Source',
  intervalHint: 'Use an interval such as 30m, 12h, or 1d.',
}

export function useServerT() {
  const { locale } = useI18n()
  return (key: Key, values: Record<string, string | number> = {}) => Object.entries(values).reduce(
    (message, [name, value]) => message.replaceAll(`{${name}}`, String(value)),
    (locale === 'zh-CN' ? zh : en)[key],
  )
}
