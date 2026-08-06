import { z } from "zod";

// ============================================
// DNS 配置（可在每个订阅中自定义）
// ============================================

/** 表单级别的通用 DNS 设置，用于自动生成各格式的 DNS 段 */
export const DnsSharedConfigSchema = z.object({
  /** 本地 DNS 服务器地址（用于 CN 域名解析），默认 "local" */
  localDns: z.string().optional(),
  /** 本地 DNS 端口，默认 53 */
  localDnsPort: z.number().int().min(1).max(65535).optional(),
  /** FakeIP IPv4 范围，默认 "198.18.0.0/15" */
  fakeipIpv4Range: z.string().optional(),
  /** FakeIP IPv6 范围，默认 "fc00::/18" */
  fakeipIpv6Range: z.string().optional(),
  /** 是否启用 FakeIP，默认 true */
  fakeipEnabled: z.boolean().optional(),
  /** FakeIP rewrite TTL（秒），默认 300 */
  fakeipTtl: z.number().int().min(0).optional(),
  bootstrapDns: z.string().optional(),
  bootstrapDnsPort: z.number().int().min(1).max(65535).optional(),
  bootstrapServerName: z.string().optional(),
  remoteDns: z.string().optional(),
  remoteDnsPort: z.number().int().min(1).max(65535).optional(),
  remoteServerName: z.string().optional(),
  remoteDetour: z.string().optional(),
  preferIpv4: z.boolean().optional(),
  /** 是否拦截 HTTPS DNS 查询，默认 true */
  rejectHttps: z.boolean().optional(),
  /** CN 域名走本地 DNS，默认 true */
  cnDomainLocalDns: z.boolean().optional(),
  /** Linux system daemon: point /etc/resolv.conf at Sempre's local DNS listener */
  systemDnsTakeoverEnabled: z.boolean().optional(),
  /** Linux system daemon DNS listener port, default 53 */
  systemDnsListenPort: z.number().int().min(1).max(65535).optional(),
});

export type DnsSharedConfig = z.infer<typeof DnsSharedConfigSchema>;

/**
 * DNS 配置存储结构
 * - shared: 表单设置，自动生成各格式的 DNS 段
 * - overrides: 原生 DNS 配置，直接透传到输出（优先于 shared 生成的内容）
 *   各 key 可存放对应格式的原生 dns 配置 JSON，
 *   singboxV12 未设置时 fallback 到 singbox，clashMeta 未设置时 fallback 到 clash
 */
export const DnsConfigSchema = z.object({
  shared: DnsSharedConfigSchema.optional(),
	modes: z.record(z.string(), z.enum(["managed", "native"])).optional(),
	overrides: z.record(z.string(), z.record(z.string(), z.unknown())).optional(),
});

export type DnsConfig = z.infer<typeof DnsConfigSchema>;

// ============================================
// 代理组定义
// ============================================

export const ProxyGroupSchema = z.object({
  name: z.string(),
  type: z.string(),
  proxies: z.array(z.string()),
  /** 这个组不加节点 */
  readonly: z.boolean().optional(),
});

export type ProxyGroup = z.infer<typeof ProxyGroupSchema>;

// ============================================
// 规则提供者
// ============================================

export const ProxyRuleProviderSchema = z.object({
  name: z.string(),
  url: z.string(),
  type: z.string().optional(),
});

export type ProxyRuleProvider = z.infer<typeof ProxyRuleProviderSchema>;

export const ProxyRuleProvidersListSchema = z.record(
  z.string(),
  z.array(ProxyRuleProviderSchema),
);

export type ProxyRuleProvidersList = z.infer<
  typeof ProxyRuleProvidersListSchema
>;

// ============================================
// 订阅源条目（结构化）
// ============================================

export const ProxySourceFetchModeSchema = z.enum(["auto", "domestic-direct"]);

export type ProxySourceFetchMode = z.infer<typeof ProxySourceFetchModeSchema>;

export const SubscribeItemSchema = z.object({
  /** Sempre stable source identifier. */
  id: z.string().optional(),
  /** 是否启用 */
  enabled: z.boolean(),
  /** 订阅地址 */
  url: z.string(),
  /** 前缀（拼接到节点名称前） */
  prefix: z.string(),
  /** 备注（允许多行） */
  remark: z.string(),
  /** 缓存时间（分钟），0 或 undefined 表示不缓存 */
  cacheTtlMinutes: z.number().min(0).optional(),
  /** 自定义 User-Agent（留空使用默认值 clash.meta） */
  fetchUa: z.string().optional(),
  /** 抓取链路（留空表示跟随系统境内外分流） */
  fetchMode: ProxySourceFetchModeSchema.optional(),
});

export type SubscribeItem = z.infer<typeof SubscribeItemSchema>;

export const SubscribeItemsSchema = z.array(SubscribeItemSchema);

// ============================================
// 代理订阅
// ============================================

export const ProxyLogLevelSchema = z.enum([
  "off",
  "error",
  "warn",
  "info",
  "debug",
]);

export type ProxyLogLevel = z.infer<typeof ProxyLogLevelSchema>;

export const ProxySubscribeSchema = z.object({
  id: z.string(),
  userId: z.string(),
  url: z.string(),
  remark: z.string().nullable(),
  logLevel: ProxyLogLevelSchema,
  // JSONC 字符串（前端编辑器直接显示）— 旧字段，保留兼容
  subscribeUrl: z.string().nullable(),
  // 结构化订阅源列表（新字段，优先使用）
  subscribeItems: z.array(SubscribeItemSchema).nullable(),
  ruleList: z.string().nullable(),
  useSystemRuleList: z.boolean(),
  group: z.string().nullable(),
  useSystemGroup: z.boolean(),
  filter: z.string().nullable(),
  useSystemFilter: z.boolean(),
  servers: z.string().nullable(),
  customConfig: z.string().nullable(),
  useSystemCustomConfig: z.boolean(),
  dnsConfig: z.string().nullable(),
  useSystemDnsConfig: z.boolean(),
  privateAccessConfig: z.string().nullable(),
  assignedCustomNodes: z.array(
    z.object({
      id: z.string(),
      userId: z.string(),
      name: z.string(),
      proxyType: z.string(),
      server: z.string(),
      port: z.number(),
      enabled: z.boolean(),
      position: z.number(),
    }),
  ),
  selectedCustomNodeIds: z.array(z.string()),
  authorizedUserIds: z.array(z.string()),
  /** 订阅缓存时间（分钟），null 或 0 表示不缓存 */
  cacheTtlMinutes: z.number().nullable(),
  lastAccessAt: z.string().nullable(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export type ProxySubscribe = z.infer<typeof ProxySubscribeSchema>;

// 用于 API 返回的完整订阅对象，包含用户信息
export const ProxySubscribeWithUserSchema = ProxySubscribeSchema.extend({
  user: z.object({
    id: z.string(),
    name: z.string(),
    email: z.string(),
  }),
  authorizedUsers: z.array(
    z.object({
      id: z.string(),
      name: z.string(),
      email: z.string(),
    }),
  ),
});

export type ProxySubscribeWithUser = z.infer<
  typeof ProxySubscribeWithUserSchema
>;

// ============================================
// 创建/更新订阅输入（JSONC 字符串）
// ============================================

export const CreateProxySubscribeInputSchema = z.object({
  remark: z.string().nullable().optional(),
  logLevel: ProxyLogLevelSchema.optional(),
  subscribeUrl: z.string().nullable().optional(),
  subscribeItems: z.array(SubscribeItemSchema).nullable().optional(),
  ruleList: z.string().nullable().optional(),
  useSystemRuleList: z.boolean().optional(),
  group: z.string().nullable().optional(),
  useSystemGroup: z.boolean().optional(),
  filter: z.string().nullable().optional(),
  useSystemFilter: z.boolean().optional(),
  servers: z.string().nullable().optional(),
  customConfig: z.string().nullable().optional(),
  useSystemCustomConfig: z.boolean().optional(),
  dnsConfig: z.string().nullable().optional(),
  useSystemDnsConfig: z.boolean().optional(),
  privateAccessConfig: z.string().nullable().optional(),
  authorizedUserIds: z.array(z.string()).optional().default([]),
  cacheTtlMinutes: z.number().min(0).nullable().optional(),
  selectedCustomNodeIds: z.array(z.string()).optional(),
});

export type CreateProxySubscribeInput = z.infer<
  typeof CreateProxySubscribeInputSchema
>;

export const UpdateProxySubscribeInputSchema = z.object({
  id: z.string(),
  remark: z.string().nullable().optional(),
  logLevel: ProxyLogLevelSchema.optional(),
  subscribeUrl: z.string().nullable().optional(),
  subscribeItems: z.array(SubscribeItemSchema).nullable().optional(),
  ruleList: z.string().nullable().optional(),
  useSystemRuleList: z.boolean().optional(),
  group: z.string().nullable().optional(),
  useSystemGroup: z.boolean().optional(),
  filter: z.string().nullable().optional(),
  useSystemFilter: z.boolean().optional(),
  servers: z.string().nullable().optional(),
  customConfig: z.string().nullable().optional(),
  useSystemCustomConfig: z.boolean().optional(),
  dnsConfig: z.string().nullable().optional(),
  useSystemDnsConfig: z.boolean().optional(),
  privateAccessConfig: z.string().nullable().optional(),
  authorizedUserIds: z.array(z.string()).optional(),
  cacheTtlMinutes: z.number().min(0).nullable().optional(),
  selectedCustomNodeIds: z.array(z.string()).optional(),
});

export type UpdateProxySubscribeInput = z.infer<
  typeof UpdateProxySubscribeInputSchema
>;

export const DeleteProxySubscribeInputSchema = z.object({
  id: z.string(),
});

export type DeleteProxySubscribeInput = z.infer<
  typeof DeleteProxySubscribeInputSchema
>;

// ============================================
// 规则测试
// ============================================

export const ProxyRuleTestInputSchema = z.object({
  url: z.string(),
});

export type ProxyRuleTestInput = z.infer<typeof ProxyRuleTestInputSchema>;

// ============================================
// 节点预览
// ============================================

/** 预览节点的基本信息 */
export const ProxyPreviewNodeSchema = z.object({
  /** 节点名称 */
  name: z.string(),
  /** 代理协议类型 (vmess, vless, ss, trojan, hysteria2 等) */
  type: z.string(),
  /** 服务器地址 */
  server: z.string(),
  /** 端口 */
  port: z.number(),
  /** 来源索引（订阅源的序号，从 1 开始） */
  sourceIndex: z.number(),
  /** 来源地址（订阅 URL） */
  sourceUrl: z.string(),
  /** 完整的代理配置（用于展示详细信息） */
  raw: z.record(z.string(), z.unknown()),
  /** 是否被过滤规则过滤 */
  filtered: z.boolean().optional(),
  /** 匹配的过滤规则 */
  filteredBy: z.string().optional(),
});

export type ProxyPreviewNode = z.infer<typeof ProxyPreviewNodeSchema>;

export const ProxyPreviewInputSchema = z.object({
  id: z.string(),
});

export type ProxyPreviewInput = z.infer<typeof ProxyPreviewInputSchema>;

export const ProxyPreviewOutputSchema = z.object({
  nodes: z.array(ProxyPreviewNodeSchema),
});

export type ProxyPreviewOutput = z.infer<typeof ProxyPreviewOutputSchema>;

// ============================================
// 订阅调试（流式）
// ============================================

/** 调试目标格式 */
export const ProxyDebugFormatSchema = z.enum([
  "clash",
  "clash-meta",
  "sing-box",
  "sing-box-windows",
  "sing-box-macos",
  "sing-box-v12",
  "sing-box-v12-windows",
  "sing-box-v12-macos",
  "sing-box-v13",
  "sing-box-v13-windows",
  "sing-box-v13-macos",
]);

export type ProxyDebugFormat = z.infer<typeof ProxyDebugFormatSchema>;

/** 调试输入 */
export const ProxyDebugInputSchema = z.object({
  id: z.string(),
  format: ProxyDebugFormatSchema,
});

export type ProxyDebugInput = z.infer<typeof ProxyDebugInputSchema>;

/** 被过滤的节点信息 */
export const ProxyDebugFilteredNodeSchema = z.object({
  node: ProxyPreviewNodeSchema,
  matchedRule: z.string(),
});

export type ProxyDebugFilteredNode = z.infer<
  typeof ProxyDebugFilteredNodeSchema
>;

/** Step: 配置解析完成 */
export const ProxyDebugConfigStepSchema = z.object({
  type: z.literal("config"),
  data: z.object({
    subscribeUrls: z.array(z.string()),
    filters: z.array(z.string()),
    groups: z.array(ProxyGroupSchema),
    ruleProviders: ProxyRuleProvidersListSchema,
    customConfig: z.array(z.unknown()),
    servers: z.array(z.unknown()),
    privateAccessConfig: z.record(z.string(), z.unknown()).nullable(),
    dnsConfig: z.object({
      shared: z.record(z.string(), z.unknown()),
      overrides: z.record(z.string(), z.unknown()),
    }),
  }),
});

/** Step: 手动服务器解析完成 */
export const ProxyDebugManualServersStepSchema = z.object({
  type: z.literal("manual-servers"),
  data: z.object({
    count: z.number(),
    nodes: z.array(ProxyPreviewNodeSchema),
  }),
});

/** Step: 开始获取远程订阅源 */
export const ProxyDebugSourceStartStepSchema = z.object({
  type: z.literal("source-start"),
  data: z.object({
    sourceIndex: z.number(),
    url: z.string(),
  }),
});

/** Step: 远程订阅源获取完成 */
export const ProxyDebugSourceResultStepSchema = z.object({
  type: z.literal("source-result"),
  data: z.object({
    sourceIndex: z.number(),
    url: z.string(),
    httpStatus: z.number().nullable(),
    httpHeaders: z.record(z.string(), z.string()),
    rawText: z.string(),
    decodedText: z.string().nullable().optional(),
    format: z.enum(["base64", "yaml", "unknown"]),
    parsedNodeCount: z.number(),
    nodesBeforeFilter: z.array(ProxyPreviewNodeSchema),
    nodesAfterFilter: z.array(ProxyPreviewNodeSchema),
    filteredNodes: z.array(ProxyDebugFilteredNodeSchema),
    error: z.string().nullable(),
    fetchDurationMs: z.number(),
    /** 是否命中缓存 */
    cached: z.boolean(),
  }),
});

/** Step: 节点合并完成 */
export const ProxyDebugMergeStepSchema = z.object({
  type: z.literal("merge"),
  data: z.object({
    totalNodesBeforeFilter: z.number(),
    totalNodesAfterFilter: z.number(),
    totalFiltered: z.number(),
    finalNodeNames: z.array(z.string()),
    /** 存在信息丢失的节点名称列表（仅 sing-box 格式） */
    nodeWarnings: z.array(z.string()).optional(),
    /** 存在忽略字段（非丢失）的节点名称列表（仅 sing-box 格式） */
    nodeIgnored: z.array(z.string()).optional(),
  }),
});

/** Step: 配置组装完成 */
export const ProxyDebugOutputStepSchema = z.object({
  type: z.literal("output"),
  data: z.object({
    proxyGroupCount: z.number(),
    ruleCount: z.number(),
    ruleProviderCount: z.number(),
    configOutput: z.string(),
  }),
});

/** Step: 全部完成 */
export const ProxyDebugDoneStepSchema = z.object({
  type: z.literal("done"),
  data: z.object({
    totalDurationMs: z.number(),
  }),
});

/** Step: 配置校验 */
export const ProxyDebugValidateStepSchema = z.object({
  type: z.literal("validate"),
  data: z.object({
    /** 校验是否通过 */
    valid: z.boolean().optional(),
    /** 校验方法 */
    method: z.string().optional(),
    /** 警告列表 */
    warnings: z.array(z.string()).optional(),
    /** 错误列表 */
    errors: z.array(z.string()).optional(),
    /** 是否跳过校验 */
    skipped: z.boolean().optional(),
    /** 跳过原因 */
    reason: z.string().optional(),
  }),
});

/** Step: 规则集调试 */
export const ProxyDebugRuleSetItemSchema = z.object({
  /** 规则集名称/tag */
  tag: z.string(),
  /** 原始 URL（规则源文件） */
  url: z.string(),
  /** 最终配置中的实际 URL（sing-box 可能是 convert 端点） */
  effectiveUrl: z.string().optional(),
  /** 所属代理分组 */
  group: z.string(),
  /** 拉取状态 */
  status: z.enum(["ok", "error", "skipped"]),
  /** 错误信息 */
  error: z.string().optional(),
  /** HTTP 状态码 */
  httpStatus: z.number().optional(),
  /** 规则条数 */
  ruleCount: z.number(),
  /** 规则样本（截断） */
  sampleRules: z.array(z.string()).optional(),
  /** 规则是否被截断 */
  truncated: z.boolean().optional(),
  /** 是否为内置规则集（如 geoip-cn） */
  builtin: z.boolean().optional(),
  /** 规则集格式（source / binary） */
  format: z.string().optional(),
});

export type ProxyDebugRuleSetItem = z.infer<typeof ProxyDebugRuleSetItemSchema>;

export const ProxyDebugRuleSetsStepSchema = z.object({
  type: z.literal("rule-sets"),
  data: z.object({
    totalCount: z.number(),
    totalRules: z.number(),
    errorCount: z.number(),
    items: z.array(ProxyDebugRuleSetItemSchema),
  }),
});

/** 调试步骤联合类型 */
export const ProxyDebugStepSchema = z.discriminatedUnion("type", [
  ProxyDebugConfigStepSchema,
  ProxyDebugManualServersStepSchema,
  ProxyDebugSourceStartStepSchema,
  ProxyDebugSourceResultStepSchema,
  ProxyDebugMergeStepSchema,
  ProxyDebugOutputStepSchema,
  ProxyDebugRuleSetsStepSchema,
  ProxyDebugValidateStepSchema,
  ProxyDebugDoneStepSchema,
]);

export type ProxyDebugStep = z.infer<typeof ProxyDebugStepSchema>;

// ============================================
// 单订阅源调试（流式）
// ============================================

export const ProxySourceDebugModeSchema = z.enum([
  "bypass-cache",
  "production",
]);

export type ProxySourceDebugMode = z.infer<typeof ProxySourceDebugModeSchema>;

export const ProxySourceDebugInputSchema = z.object({
  url: z.string(),
  ua: z.string().optional(),
  prefix: z.string().optional(),
  cacheTtlMinutes: z.number().min(0).optional(),
  mode: ProxySourceDebugModeSchema,
  fetchMode: ProxySourceFetchModeSchema.default("auto"),
});

export type ProxySourceDebugInput = z.infer<typeof ProxySourceDebugInputSchema>;

export const ProxySourceDebugPayloadSchema = z.object({
  format: z.enum(["base64", "yaml", "unknown"]),
  rawText: z.string(),
  decodedText: z.string().nullable(),
  bodyBytes: z.number(),
  parsedNodeCount: z.number(),
  nodes: z.array(ProxyPreviewNodeSchema),
  discardedPlaceholderNodes: z.array(ProxyPreviewNodeSchema),
  diagnostics: z.array(z.string()),
});

export type ProxySourceDebugPayload = z.infer<
  typeof ProxySourceDebugPayloadSchema
>;

export const ProxySourceDebugConfigStepSchema = z.object({
  type: z.literal("config"),
  data: z.object({
    url: z.string(),
    ua: z.string(),
    prefix: z.string(),
    cacheTtlMinutes: z.number(),
    mode: ProxySourceDebugModeSchema,
    fetchMode: ProxySourceFetchModeSchema,
    proxyEndpoint: z.string().nullable(),
    maxAttempts: z.number(),
    timeoutMs: z.number(),
  }),
});

export const ProxySourceDebugCacheStepSchema = z.object({
  type: z.literal("cache"),
  data: z.object({
    status: z.enum(["skipped", "miss", "expired", "hit", "unusable"]),
    cacheTtlMinutes: z.number(),
    payload: ProxySourceDebugPayloadSchema.nullable(),
  }),
});

export const ProxySourceDebugAttemptStartStepSchema = z.object({
  type: z.literal("attempt-start"),
  data: z.object({
    attempt: z.number(),
    maxAttempts: z.number(),
  }),
});

export const ProxySourceDebugNetworkStepSchema = z.object({
  type: z.literal("network"),
  data: z.object({
    fetchMode: ProxySourceFetchModeSchema,
    connectionKind: z.enum(["origin", "proxy"]),
    proxyEndpoint: z.string().nullable(),
    scheme: z.string().nullable(),
    host: z.string().nullable(),
    port: z.number().nullable(),
    resolverConfig: z.array(z.string()),
    proxyEnvironmentVariables: z.array(z.string()),
    dnsDurationMs: z.number(),
    resolvedAddresses: z.array(z.string()),
    dnsError: z.string().nullable(),
    tcpProbes: z.array(
      z.object({
        address: z.string(),
        success: z.boolean(),
        durationMs: z.number(),
        localAddress: z.string().nullable(),
        remoteAddress: z.string().nullable(),
        error: z.string().nullable(),
      }),
    ),
  }),
});

export const ProxySourceDebugRequestErrorSchema = z.object({
  message: z.string(),
  debug: z.string(),
  chain: z.array(z.string()),
  isTimeout: z.boolean(),
  isConnect: z.boolean(),
  isRequest: z.boolean(),
  isBody: z.boolean(),
  isDecode: z.boolean(),
  status: z.number().nullable(),
  url: z.string().nullable(),
});

export const ProxySourceDebugAttemptResultStepSchema = z.object({
  type: z.literal("attempt-result"),
  data: z.object({
    attempt: z.number(),
    maxAttempts: z.number(),
    success: z.boolean(),
    httpStatus: z.number().nullable(),
    finalUrl: z.string().nullable(),
    httpHeaders: z.record(z.string(), z.string()),
    fetchDurationMs: z.number(),
    error: z.string().nullable(),
    requestError: ProxySourceDebugRequestErrorSchema.nullable(),
    remoteAddress: z.string().nullable(),
    httpVersion: z.string().nullable(),
    tlsPeerCertificateBytes: z.number().nullable(),
    payload: ProxySourceDebugPayloadSchema,
  }),
});

export const ProxySourceDebugFallbackStepSchema = z.object({
  type: z.literal("fallback"),
  data: z.object({
    status: z.enum(["hit", "miss", "unusable"]),
    payload: ProxySourceDebugPayloadSchema.nullable(),
  }),
});

export const ProxySourceDebugDoneStepSchema = z.object({
  type: z.literal("done"),
  data: z.object({
    success: z.boolean(),
    resultSource: z.enum(["cache", "live", "stale-cache"]).nullable(),
    nodeCount: z.number(),
    totalDurationMs: z.number(),
  }),
});

export const ProxySourceDebugStepSchema = z.discriminatedUnion("type", [
  ProxySourceDebugConfigStepSchema,
  ProxySourceDebugCacheStepSchema,
  ProxySourceDebugNetworkStepSchema,
  ProxySourceDebugAttemptStartStepSchema,
  ProxySourceDebugAttemptResultStepSchema,
  ProxySourceDebugFallbackStepSchema,
  ProxySourceDebugDoneStepSchema,
]);

export type ProxySourceDebugStep = z.infer<typeof ProxySourceDebugStepSchema>;

// ============================================
// 节点链路追踪
// ============================================

/** 节点追踪输入 */
export const ProxyNodeTraceInputSchema = z.object({
  /** 订阅 ID */
  id: z.string(),
  /** 输出格式 */
  format: ProxyDebugFormatSchema,
  /** 节点名称（appendIcon 后的名称） */
  nodeName: z.string(),
});

export type ProxyNodeTraceInput = z.infer<typeof ProxyNodeTraceInputSchema>;

/** 追踪步骤: 来源 */
export const ProxyNodeTraceSourceStepSchema = z.object({
  type: z.literal("source"),
  data: z.object({
    /** 来源索引（0=手动，1+=远程订阅源序号） */
    sourceIndex: z.number(),
    /** 来源地址 */
    sourceUrl: z.string(),
    /** 来源格式 */
    format: z.enum(["base64", "yaml", "manual"]),
    /** 原始代理 URI（仅 Base64 订阅源，如 vless://...、vmess://... 等） */
    rawUrl: z.string().optional(),
    /** 原始代理配置数据（来自上游的原始数据） */
    rawData: z.record(z.string(), z.unknown()),
  }),
});

/** 追踪步骤: 解析为 Clash proxy */
export const ProxyNodeTraceParseStepSchema = z.object({
  type: z.literal("parse"),
  data: z.object({
    /** 解析后的 Clash proxy 对象 */
    clashProxy: z.record(z.string(), z.unknown()),
  }),
});

/** 追踪步骤: 过滤检查 */
export const ProxyNodeTraceFilterStepSchema = z.object({
  type: z.literal("filter"),
  data: z.object({
    /** 是否通过过滤 */
    passed: z.boolean(),
    /** 匹配到的过滤规则（被过滤时） */
    matchedRule: z.string().nullable(),
    /** 应用的所有过滤规则 */
    filtersApplied: z.array(z.string()),
  }),
});

/** 追踪步骤: 名称富化 */
export const ProxyNodeTraceEnrichStepSchema = z.object({
  type: z.literal("enrich"),
  data: z.object({
    /** 原始名称 */
    originalName: z.string(),
    /** 富化后名称（appendIcon 后） */
    enrichedName: z.string(),
  }),
});

/** 追踪步骤: 合并 */
export const ProxyNodeTraceMergeStepSchema = z.object({
  type: z.literal("merge"),
  data: z.object({
    /** 在最终列表中的位置（从 1 开始） */
    positionInFinalList: z.number(),
    /** 最终列表总节点数 */
    totalNodes: z.number(),
  }),
});

/** 追踪步骤: 分组分配 */
export const ProxyNodeTraceGroupAssignStepSchema = z.object({
  type: z.literal("group-assign"),
  data: z.object({
    /** 被分配到的分组列表 */
    assignedGroups: z.array(
      z.object({
        name: z.string(),
        type: z.string(),
      }),
    ),
  }),
});

/** 字段溯源信息 */
export const FieldOriginSchema = z.object({
  /** 源字段名（Clash 侧），null 表示生成字段 */
  sourceKey: z.string().nullable(),
  /** 源字段值 */
  sourceValue: z.unknown().optional(),
  /** 转换步骤: core | tls | transport | multiplex | dial | type | unknown */
  step: z.string(),
  /** 变换类型: direct | rename | convert | extract | generated | fallback | container */
  transform: z.string(),
  /** 生成字段的原因代码 */
  reason: z.string().optional(),
  /** 容器节点的源字段列表 */
  sources: z.array(z.string()).optional(),
});

export type FieldOrigin = z.infer<typeof FieldOriginSchema>;

/** 追踪步骤: 格式转换（仅 Sing-box） */
export const ProxyNodeTraceConvertStepSchema = z.object({
  type: z.literal("convert"),
  data: z.object({
    /** 转换后的 Sing-box outbound 对象 */
    singboxOutbound: z.record(z.string(), z.unknown()),
    /** 转换过程中丢失的配置字段名列表（真实数据丢失） */
    lostFields: z.array(z.string()).optional(),
    /** 转换中有意忽略的字段（目标格式不适用，非数据丢失） */
    ignoredFields: z.array(z.string()).optional(),
    /** 字段溯源映射: dot-path → 溯源信息 */
    fieldOrigins: z.record(z.string(), FieldOriginSchema).optional(),
  }),
});

/** 追踪步骤: 最终输出 */
export const ProxyNodeTraceOutputStepSchema = z.object({
  type: z.literal("output"),
  data: z.object({
    /** 该节点在最终配置中的片段（YAML 或 JSON） */
    configFragment: z.string(),
  }),
});

/** 追踪步骤联合类型 */
export const ProxyNodeTraceStepSchema = z.discriminatedUnion("type", [
  ProxyNodeTraceSourceStepSchema,
  ProxyNodeTraceParseStepSchema,
  ProxyNodeTraceFilterStepSchema,
  ProxyNodeTraceEnrichStepSchema,
  ProxyNodeTraceMergeStepSchema,
  ProxyNodeTraceGroupAssignStepSchema,
  ProxyNodeTraceConvertStepSchema,
  ProxyNodeTraceOutputStepSchema,
]);

export type ProxyNodeTraceStep = z.infer<typeof ProxyNodeTraceStepSchema>;

/** 节点追踪输出 */
export const ProxyNodeTraceOutputSchema = z.object({
  /** 追踪的节点名称 */
  nodeName: z.string(),
  /** 追踪步骤列表 */
  steps: z.array(ProxyNodeTraceStepSchema),
});

export type ProxyNodeTraceOutput = z.infer<typeof ProxyNodeTraceOutputSchema>;
