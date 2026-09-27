export interface Target {
  core: string
  format: string
  version: string
  platform: string
  standalone: boolean
}

export interface PreviewNode {
  name: string
  type: string
  server: string
  port: number
  sourceIndex: number
  sourceUrl: string
  raw: Record<string, unknown>
  filtered?: boolean
  filteredBy?: string
}

export interface AccessStats {
  totalAccesses: number
  todayAccess: number
  cachedNodeCount: number
  lastAccessAt: string | null
  accessByType: { type: string; count: number }[]
  recentAccessTotal: number
  recentAccesses: {
    id: string
    subscribeId: string
    accessType: string
    ip: string | null
    userAgent: string | null
    nodeCount: number | null
    createdAt: string
  }[]
}

export interface DebugStage {
  type: string
  status: 'running' | 'ok' | 'skipped' | 'error'
  sourceId?: string
  cached?: boolean
  cacheState?: 'fresh' | 'stale' | 'miss' | 'bypass'
  message?: string
}

export interface DraftDebugResult {
  ok: boolean
  format?: string
  content?: string
  nodeCount?: number
  diagnostics: { level: string; sourceId?: string; message: string }[]
  fieldDiffs?: unknown
  decoded?: unknown
  nodeOrigins?: unknown
  stages: DebugStage[]
  elapsedMs: number
}

export interface SourceDebugResult {
  ok: boolean
  status: number | null
  ua: string
  nodeCount: number
  nodes: { name: string; type: string; server: string; port: number }[]
  elapsedMs: number
  bodyBytes: number
  cached: boolean
  cacheState: 'fresh' | 'stale' | 'miss' | 'bypass'
  warning?: string
  diagnostics: unknown
  responseHeaders: Record<string, string>
  raw: string
  rawTruncated: boolean
  decodedText: string
  decodedTextTruncated: boolean
  decoded: unknown[]
  stages: DebugStage[]
}

export interface NodeTraceResult {
  nodeName: string
  steps: { type: string; data: unknown }[]
}

export function targetSuffix(format: string): string | null {
  if (['clash', 'clash-meta', 'clash-rs', 'xray', 'v2ray', 'dae'].includes(format)) return format
  const match = /^sing-box(?:-v(12|13|14))?(?:-(windows|macos))?$/.exec(format)
  if (!match) return null
  return ['sing-box', match[1], match[2]].filter(Boolean).join('/')
}
