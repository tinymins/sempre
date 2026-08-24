import { z } from "zod";

// ============================================
// DNS 配置（可在每个订阅中自定义）
// ============================================

/** 表单级别的通用 DNS 设置，用于自动生成各格式的 DNS 段 */
export const DnsSharedConfigSchema = z.object({
  /** 本地 DNS 传输方式，默认 UDP */
  localDnsTransport: z.enum(["system", "udp", "tls"]).optional(),
  /** 本地 DNS 服务器地址（用于 CN 域名解析），默认 223.5.5.5 */
  localDns: z.string().optional(),
  /** 本地 DNS 端口，默认 53 */
  localDnsPort: z.number().int().min(1).max(65535).optional(),
  localServerName: z.string().optional(),
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
  cnIpLocalDns: z.boolean().optional(),
  excludeHkFromCnIp: z.boolean().optional(),
  cnDomainRuleSetEnabled: z.boolean().optional(),
  cnDomainRuleSetUrl: z.string().optional(),
  cnDomainRuleSetDetour: z.string().optional(),
  cnIpRuleSetEnabled: z.boolean().optional(),
  cnIpRuleSetUrl: z.string().optional(),
  cnIpRuleSetDetour: z.string().optional(),
  hkIpRuleSetEnabled: z.boolean().optional(),
  hkIpRuleSetUrl: z.string().optional(),
  hkIpRuleSetDetour: z.string().optional(),
  /** Linux system daemon: point /etc/resolv.conf at Sempre's local DNS listener */
  systemDnsTakeoverEnabled: z.boolean().optional(),
  /** Linux system daemon DNS listener port, default 53 */
  systemDnsListenPort: z.number().int().min(1).max(65535).optional(),
  /** Linux system daemon DNS listener hosts, default ["127.0.0.1"] */
  systemDnsListenHosts: z.array(z.string()).optional(),
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
