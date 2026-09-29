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

export const DnsConfigSchema = z.object({
  shared: DnsSharedConfigSchema.optional(),
});

export type DnsConfig = z.infer<typeof DnsConfigSchema>;


import { editJsonc } from "./jsonc";
export const updateDnsField = (source: string | undefined, field: keyof DnsSharedConfig, value: unknown) => editJsonc(source, ["shared", field], value);
