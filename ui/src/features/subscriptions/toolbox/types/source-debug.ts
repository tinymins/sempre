import { z } from "zod";
import { ProxyPreviewNodeSchema } from "./proxy-debug";
import { ProxySourceFetchModeSchema } from "./subscription";

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
