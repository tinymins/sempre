import { z } from "zod";
import { ProxyDebugFormatSchema } from "./proxy-debug";

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
