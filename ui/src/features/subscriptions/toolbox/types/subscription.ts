import { z } from "zod";
export { DnsConfigSchema, DnsSharedConfigSchema } from "@acme/subscription-editor";
export type { DnsConfig, DnsSharedConfig } from "@acme/subscription-editor";

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
