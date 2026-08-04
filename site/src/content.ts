export type Locale = 'en' | 'zh-CN'

export const copy = {
  en: {
    skip: 'Skip to content', navArchitecture: 'Architecture', navPlatforms: 'Platforms', navDocs: 'Docs',
    eyebrow: 'Native lifecycle control plane', tagline: 'Any core. Always current. Always running.',
    lead: 'Install, switch, validate, and supervise proxy cores with one native service and one replaceable Web UI.',
    quickStart: 'Quick start', oneCommand: 'One command, verified release.', checksum: 'SHA-256 verified before execution',
    inspectScript: 'Inspect script', releaseNote: 'Pre-1.0 software · unsigned binaries · review release notes before privileged installation',
    signalNative: 'Native service managers', signalAtomic: 'Atomic core switching', signalReplaceable: 'Replaceable Web UI', signalCrossPlatform: 'Three operating systems',
    controlTitle: 'The control plane stays online while the core changes underneath it.',
    controlLead: 'Observe runtime state, traffic, connections, rules, logs, and installed versions from the same local interface.',
    controlAlt: 'Sempre control plane overview', realInterface: 'Sempre Web UI · actual interface',
    architectureTitle: 'One ownership chain. No wrapper stack.',
    architectureLead: 'A single Go binary owns installation, service registration, validation, deployment, supervision, and rollback.',
    interfaceNode: 'Operator interface', supervisorNode: 'Supervisor + rollback', coreNode: 'Managed process',
    nativeDetail: 'Registered directly with each operating system. No NSSM, no third-party service host.',
    platformTitle: 'Same install contract on every supported machine.',
    platformLead: "The installer detects the machine, locks the latest release tag, verifies the matching bundle, then delegates to Sempre's idempotent installer.",
    ready: 'Ready', trustTitle: 'The short command does not shorten the verification chain.',
    trustLead: 'Every target ships with checksums, a CycloneDX SBOM, and GitHub build provenance. Bundle verification happens before the privileged installer starts.',
    viewRelease: 'View the latest release', footerTagline: 'Any core. Always current. Always running.', releases: 'Releases', security: 'Security',
    independent: 'Independent community project. Not affiliated with proxy core projects.', copied: 'Install command copied', copyCommand: 'Copy install command',
    theme: 'Theme', themeSystem: 'System', themeLight: 'Light', themeDark: 'Dark',
    meta: 'Sempre installs, updates, switches, and supervises proxy cores as a native system service.',
  },
  'zh-CN': {
    skip: '跳到正文', navArchitecture: '架构', navPlatforms: '平台', navDocs: '文档',
    eyebrow: '原生核心生命周期控制面', tagline: '任意核心，持续更新，始终运行。',
    lead: '用一个原生系统服务和一套可替换 Web UI，完成代理核心的安装、切换、验证与守护。',
    quickStart: '快速开始', oneCommand: '一行命令，校验后安装。', checksum: '执行前完成 SHA-256 校验',
    inspectScript: '审阅脚本', releaseNote: '1.0 前版本 · 二进制暂未签名 · 执行特权安装前请阅读发行说明',
    signalNative: '原生服务管理器', signalAtomic: '原子核心切换', signalReplaceable: '可替换 Web UI', signalCrossPlatform: '三个操作系统',
    controlTitle: '底层核心持续变化，控制面始终在线。',
    controlLead: '在同一个本地界面查看运行状态、流量、连接、规则、日志和已安装版本。',
    controlAlt: 'Sempre 控制面概览', realInterface: 'Sempre Web UI · 真实产品界面',
    architectureTitle: '一条清晰的所有权链，不叠加包装器。',
    architectureLead: '一个 Go 二进制负责安装、服务注册、验证、部署、守护和回滚。',
    interfaceNode: '操作入口', supervisorNode: '守护与回滚', coreNode: '受管进程',
    nativeDetail: '直接注册到各操作系统，不依赖 NSSM 或第三方服务宿主。',
    platformTitle: '每台受支持设备，遵循同一套安装契约。',
    platformLead: '安装器识别系统与架构、锁定最新发行标签、校验对应 Bundle，再交由 Sempre 的幂等安装流程完成部署。',
    ready: '支持', trustTitle: '命令可以很短，验证链不能缩短。',
    trustLead: '每个目标都提供校验和、CycloneDX SBOM 和 GitHub 构建证明；特权安装器启动前必须先通过 Bundle 校验。',
    viewRelease: '查看最新发行版', footerTagline: '任意核心，持续更新，始终运行。', releases: '发行版', security: '安全',
    independent: '独立社区项目，与各代理核心项目没有隶属关系。', copied: '安装命令已复制', copyCommand: '复制安装命令',
    theme: '主题', themeSystem: '跟随系统', themeLight: '浅色', themeDark: '深色',
    meta: 'Sempre 以原生系统服务安装、更新、切换和守护代理核心。',
  },
} as const

export type CopyKey = keyof typeof copy.en

export function resolveInitialLocale(saved: string | null, browserLanguage: string): Locale {
  if (saved === 'en' || saved === 'zh-CN') return saved
  return browserLanguage.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en'
}
