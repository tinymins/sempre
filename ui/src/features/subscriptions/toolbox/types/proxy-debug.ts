import { z } from "zod";
import { ProxyGroupSchema, ProxyRuleProvidersListSchema } from "./subscription";

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
