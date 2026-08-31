import { Checkbox, Input, InputNumber, Select, Switch } from "@acme/components";
import type { DnsConfig, DnsSharedConfig } from "@acme/types";
import { parse as parseJsonc } from "jsonc-parser";
import type React from "react";
import { useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";

const CLIENT_DEFAULT_SHARED: Required<DnsSharedConfig> = {
  localDnsTransport: "udp",
  localDns: "223.5.5.5",
  localDnsPort: 53,
  localServerName: "",
  bootstrapDns: "223.5.5.5",
  bootstrapDnsPort: 853,
  bootstrapServerName: "dns.alidns.com",
  remoteDns: "8.8.8.8",
  remoteDnsPort: 853,
  remoteServerName: "dns.google",
  remoteDetour: "",
  fakeipIpv4Range: "198.18.0.0/15",
  fakeipIpv6Range: "fc00::/18",
  fakeipEnabled: true,
  fakeipTtl: 300,
  rejectHttps: true,
  cnDomainLocalDns: true,
  cnIpLocalDns: true,
  excludeHkFromCnIp: true,
  cnDomainRuleSetEnabled: true,
  cnDomainRuleSetUrl: "https://cdn.jsdelivr.net/gh/SagerNet/sing-geosite@rule-set/geosite-cn.srs",
  cnDomainRuleSetDetour: "direct",
  cnIpRuleSetEnabled: true,
  cnIpRuleSetUrl: "https://cdn.jsdelivr.net/gh/SagerNet/sing-geoip@rule-set/geoip-cn.srs",
  cnIpRuleSetDetour: "direct",
  hkIpRuleSetEnabled: true,
  hkIpRuleSetUrl: "https://cdn.jsdelivr.net/gh/SagerNet/sing-geoip@rule-set/geoip-hk.srs",
  hkIpRuleSetDetour: "direct",
  preferIpv4: true,
  systemDnsTakeoverEnabled: false,
  systemDnsListenPort: 53,
  systemDnsListenHosts: ["127.0.0.1"],
};

export interface SystemDnsListenHostOption {
  value: string;
  label: string;
}

interface DnsConfigEditorProps {
  value?: string;
  onChange?: (value: string) => void;
  readOnly?: boolean;
  features?: string[];
  systemDnsListenHostOptions?: SystemDnsListenHostOption[];
}

const parseDnsConfig = (jsonc: string | undefined): DnsConfig => {
  if (!jsonc) return {};
  try {
    const parsed = parseJsonc(jsonc);
    return parsed && typeof parsed === "object" ? (parsed as DnsConfig) : {};
  } catch {
    return {};
  }
};

const serializeDnsConfig = (config: DnsConfig): string => {
  const result: DnsConfig = {};
  if (config.shared) {
    const diff: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(config.shared)) {
      const defaultValue = CLIENT_DEFAULT_SHARED[key as keyof Required<DnsSharedConfig>];
      if (JSON.stringify(value) !== JSON.stringify(defaultValue)) diff[key] = value;
    }
    if (Object.keys(diff).length > 0) result.shared = diff as DnsSharedConfig;
  }
  return Object.keys(result).length > 0 ? JSON.stringify(result, null, 2) : "";
};

const DnsConfigEditor = ({ value, onChange, readOnly, features = [], systemDnsListenHostOptions = [] }: DnsConfigEditorProps) => {
  const parsed = useMemo(() => parseDnsConfig(value), [value]);
  const mergedShared = useMemo(() => ({ ...CLIENT_DEFAULT_SHARED, ...(parsed.shared ?? {}) }), [parsed]);

  const update = useCallback((next: DnsConfig) => onChange?.(serializeDnsConfig(next)), [onChange]);

  const handleSharedChange = useCallback((field: keyof DnsSharedConfig, fieldValue: unknown) => {
    if (readOnly) return;
    const current = parseDnsConfig(value);
    update({ ...current, shared: { ...CLIENT_DEFAULT_SHARED, ...(current.shared ?? {}), [field]: fieldValue } });
  }, [readOnly, update, value]);

  return (
    <SharedForm merged={mergedShared} readOnly={readOnly} features={features} systemDnsListenHostOptions={systemDnsListenHostOptions} onFieldChange={handleSharedChange} />
  );
};

interface SharedFormProps {
  merged: Required<DnsSharedConfig>;
  readOnly?: boolean;
  features: string[];
  systemDnsListenHostOptions: SystemDnsListenHostOption[];
  onFieldChange: (field: keyof DnsSharedConfig, value: unknown) => void;
}

const SharedForm = ({ merged, readOnly, features, systemDnsListenHostOptions, onFieldChange }: SharedFormProps) => {
  const { t } = useTranslation();
  const disabled = readOnly ?? false;
  const supported = new Set(features);
  const listenHostOptions = useMemo(() => {
    const seen = new Set<string>();
    const result = [
      { value: "127.0.0.1", label: "127.0.0.1" },
      { value: "0.0.0.0", label: "0.0.0.0" },
      ...systemDnsListenHostOptions,
    ].filter((option) => {
      if (seen.has(option.value)) return false;
      seen.add(option.value);
      return true;
    });
    const wildcard = merged.systemDnsListenHosts.includes("0.0.0.0");
    return result.map((option) => ({ ...option, disabled: wildcard && option.value !== "0.0.0.0" }));
  }, [merged.systemDnsListenHosts, systemDnsListenHostOptions]);
  const handleListenHostsChange = (values: Array<string | number>) => {
    const hosts = values.map(String);
    if (hosts.includes("0.0.0.0")) {
      onFieldChange("systemDnsListenHosts", ["0.0.0.0"]);
      return;
    }
    onFieldChange("systemDnsListenHosts", hosts.length > 0 ? hosts : ["127.0.0.1"]);
  };
  return (
    <div className="space-y-4">
      {supported.has("dns.local_upstream") ? (
        <>
          <SectionTitle title={t("proxy.form.dnsLocalSection")} />
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            {supported.has("dns.local_transport") ? <FieldRow label={t("proxy.form.dnsLocalTransport")}><Select size="small" value={merged.localDnsTransport} disabled={disabled} options={[{ value: "udp", label: "UDP" }, { value: "tls", label: "TLS" }, { value: "system", label: t("proxy.form.dnsLocalTransportSystem") }]} onChange={(next) => onFieldChange("localDnsTransport", next)} /></FieldRow> : null}
            {merged.localDnsTransport !== "system" ? <FieldRow label={t("proxy.form.dnsLocalDns")}><Input size="small" value={merged.localDns} disabled={disabled} onChange={(event) => onFieldChange("localDns", event.target.value)} /></FieldRow> : null}
            {merged.localDnsTransport !== "system" ? <FieldRow label={t("proxy.form.dnsLocalDnsPort")}><InputNumber size="small" className="w-full" min={1} max={65535} value={merged.localDnsPort} disabled={disabled} onChange={(next) => onFieldChange("localDnsPort", next)} /></FieldRow> : null}
            {merged.localDnsTransport === "tls" ? <FieldRow label={t("proxy.form.dnsServerName")}><Input size="small" value={merged.localServerName} disabled={disabled} onChange={(event) => onFieldChange("localServerName", event.target.value)} /></FieldRow> : null}
          </div>
        </>
      ) : null}

      {supported.has("dns.bootstrap_upstream") ? (
        <>
          <SectionTitle title={t("proxy.form.dnsBootstrapSection")} />
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            <FieldRow label={t("proxy.form.dnsBootstrapDns")}><Input size="small" value={merged.bootstrapDns} disabled={disabled} onChange={(event) => onFieldChange("bootstrapDns", event.target.value)} /></FieldRow>
            {supported.has("dns.bootstrap_port") ? <FieldRow label={t("proxy.form.dnsPort")}><InputNumber size="small" className="w-full" min={1} max={65535} value={merged.bootstrapDnsPort} disabled={disabled} onChange={(next) => onFieldChange("bootstrapDnsPort", next)} /></FieldRow> : null}
            {supported.has("dns.bootstrap_server_name") ? <FieldRow label={t("proxy.form.dnsServerName")} span2><Input size="small" value={merged.bootstrapServerName} disabled={disabled} onChange={(event) => onFieldChange("bootstrapServerName", event.target.value)} /></FieldRow> : null}
          </div>
        </>
      ) : null}

      {supported.has("dns.remote_upstream") ? (
        <>
          <SectionTitle title={t("proxy.form.dnsRemoteSection")} />
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            <FieldRow label={t("proxy.form.dnsRemoteDns")}><Input size="small" value={merged.remoteDns} disabled={disabled} onChange={(event) => onFieldChange("remoteDns", event.target.value)} /></FieldRow>
			{supported.has("dns.remote_port") ? <FieldRow label={t("proxy.form.dnsPort")}><InputNumber size="small" className="w-full" min={1} max={65535} value={merged.remoteDnsPort} disabled={disabled} onChange={(next) => onFieldChange("remoteDnsPort", next)} /></FieldRow> : null}
            {supported.has("dns.remote_server_name") ? <FieldRow label={t("proxy.form.dnsServerName")}><Input size="small" value={merged.remoteServerName} disabled={disabled} onChange={(event) => onFieldChange("remoteServerName", event.target.value)} /></FieldRow> : null}
            {supported.has("dns.remote_detour") ? <FieldRow label={t("proxy.form.dnsRemoteDetour")}><Input size="small" value={merged.remoteDetour} disabled={disabled} onChange={(event) => onFieldChange("remoteDetour", event.target.value)} /></FieldRow> : null}
          </div>
        </>
      ) : null}

      {supported.has("dns.fake_ip") ? (
        <>
          <SectionTitle title="FakeIP" />
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            <FieldRow label={t("proxy.form.dnsFakeipEnabled")}><Switch size="small" checked={merged.fakeipEnabled} disabled={disabled} onChange={(next) => onFieldChange("fakeipEnabled", next)} /></FieldRow>
            <FieldRow label={t("proxy.form.dnsFakeipTtl")}><InputNumber size="small" className="w-full" min={0} value={merged.fakeipTtl} disabled={disabled} onChange={(next) => onFieldChange("fakeipTtl", next)} /></FieldRow>
            <FieldRow label={t("proxy.form.dnsFakeipIpv4Range")}><Input size="small" value={merged.fakeipIpv4Range} disabled={disabled} onChange={(event) => onFieldChange("fakeipIpv4Range", event.target.value)} /></FieldRow>
            <FieldRow label={t("proxy.form.dnsFakeipIpv6Range")}><Input size="small" value={merged.fakeipIpv6Range} disabled={disabled} onChange={(event) => onFieldChange("fakeipIpv6Range", event.target.value)} /></FieldRow>
          </div>
        </>
      ) : null}

      {supported.has("dns.reject_https") || supported.has("dns.split") || supported.has("dns.prefer_ipv4") ? (
        <>
          <SectionTitle title={t("proxy.form.dnsDnsRules")} />
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            {supported.has("dns.reject_https") ? <FieldRow label={t("proxy.form.dnsRejectHttps")}><Switch size="small" checked={merged.rejectHttps} disabled={disabled} onChange={(next) => onFieldChange("rejectHttps", next)} /></FieldRow> : null}
            {supported.has("dns.split") ? <FieldRow label={t("proxy.form.dnsCnDomainLocalDns")}><Switch size="small" checked={merged.cnDomainLocalDns} disabled={disabled} onChange={(next) => onFieldChange("cnDomainLocalDns", next)} /></FieldRow> : null}
            {supported.has("dns.geo_sources") ? <FieldRow label={t("proxy.form.dnsCnIpLocalDns")}><Switch size="small" checked={merged.cnIpLocalDns} disabled={disabled} onChange={(next) => onFieldChange("cnIpLocalDns", next)} /></FieldRow> : null}
            {supported.has("dns.geo_sources") ? <FieldRow label={t("proxy.form.dnsExcludeHkFromCnIp")}><Switch size="small" checked={merged.excludeHkFromCnIp} disabled={disabled || !merged.cnIpLocalDns} onChange={(next) => onFieldChange("excludeHkFromCnIp", next)} /></FieldRow> : null}
            {supported.has("dns.prefer_ipv4") ? <FieldRow label={t("proxy.form.dnsPreferIpv4")}><Switch size="small" checked={merged.preferIpv4} disabled={disabled} onChange={(next) => onFieldChange("preferIpv4", next)} /></FieldRow> : null}
          </div>
        </>
      ) : null}

      {supported.has("dns.geo_sources") ? (
        <>
          <SectionTitle title={t("proxy.form.dnsGeoSourcesSection")} />
          <GeoSourceFields label={t("proxy.form.dnsCnDomainRuleSet")} enabled={merged.cnDomainRuleSetEnabled} url={merged.cnDomainRuleSetUrl} detour={merged.cnDomainRuleSetDetour} disabled={disabled} onEnabledChange={(next) => onFieldChange("cnDomainRuleSetEnabled", next)} onUrlChange={(next) => onFieldChange("cnDomainRuleSetUrl", next)} onDetourChange={(next) => onFieldChange("cnDomainRuleSetDetour", next)} />
          <GeoSourceFields label={t("proxy.form.dnsCnIpRuleSet")} enabled={merged.cnIpRuleSetEnabled} url={merged.cnIpRuleSetUrl} detour={merged.cnIpRuleSetDetour} disabled={disabled} onEnabledChange={(next) => onFieldChange("cnIpRuleSetEnabled", next)} onUrlChange={(next) => onFieldChange("cnIpRuleSetUrl", next)} onDetourChange={(next) => onFieldChange("cnIpRuleSetDetour", next)} />
          <GeoSourceFields label={t("proxy.form.dnsHkIpRuleSet")} enabled={merged.hkIpRuleSetEnabled} url={merged.hkIpRuleSetUrl} detour={merged.hkIpRuleSetDetour} disabled={disabled} onEnabledChange={(next) => onFieldChange("hkIpRuleSetEnabled", next)} onUrlChange={(next) => onFieldChange("hkIpRuleSetUrl", next)} onDetourChange={(next) => onFieldChange("hkIpRuleSetDetour", next)} />
        </>
      ) : null}

      {supported.has("dns.system_takeover") ? (
        <>
          <SectionTitle title={t("proxy.form.dnsSystemSection")} />
          <p className="text-xs leading-5 text-gray-500 dark:text-gray-400">{t("proxy.form.dnsSystemDetail")}</p>
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            <FieldRow label={t("proxy.form.dnsSystemTakeover")}><Switch size="small" checked={merged.systemDnsTakeoverEnabled} disabled={disabled} onChange={(next) => onFieldChange("systemDnsTakeoverEnabled", next)} /></FieldRow>
            <FieldRow label={t("proxy.form.dnsSystemListenPort")}><InputNumber size="small" className="w-full" min={1} max={65535} value={merged.systemDnsListenPort} disabled={disabled || !merged.systemDnsTakeoverEnabled} onChange={(next) => onFieldChange("systemDnsListenPort", next)} /></FieldRow>
            <FieldRow label={t("proxy.form.dnsSystemListenHosts")} span2><Checkbox.Group options={listenHostOptions} value={merged.systemDnsListenHosts} disabled={disabled || !merged.systemDnsTakeoverEnabled} onChange={handleListenHostsChange} /></FieldRow>
          </div>
        </>
      ) : null}
    </div>
  );
};

const GeoSourceFields = ({ label, enabled, url, detour, disabled, onEnabledChange, onUrlChange, onDetourChange }: { label: string; enabled: boolean; url: string; detour: string; disabled: boolean; onEnabledChange: (value: boolean) => void; onUrlChange: (value: string) => void; onDetourChange: (value: string) => void }) => {
  const { t } = useTranslation();
  return (
    <div className="space-y-3 rounded border border-gray-200 p-3 dark:border-gray-700">
      <FieldRow label={label}><Switch size="small" checked={enabled} disabled={disabled} onChange={onEnabledChange} /></FieldRow>
      <FieldRow label={t("proxy.form.dnsGeoSourceUrl")}><Input size="small" value={url} disabled={disabled || !enabled} onChange={(event) => onUrlChange(event.target.value)} /></FieldRow>
      <FieldRow label={t("proxy.form.dnsGeoSourceDetour")}><Input size="small" value={detour} disabled={disabled || !enabled} onChange={(event) => onDetourChange(event.target.value)} /></FieldRow>
    </div>
  );
};

const SectionTitle = ({ title }: { title: string }) => <span className="block pt-1 text-xs font-semibold text-gray-500 dark:text-gray-400">{title}</span>;

const FieldRow = ({ label, children, span2 }: { label: string; children: React.ReactNode; span2?: boolean }) => (
  <div className={`flex min-w-0 items-center gap-2 ${span2 ? "md:col-span-2" : ""}`}>
    <span className="min-w-28 shrink-0 text-right text-xs text-gray-600 dark:text-gray-400">{label}</span>
    <div className="min-w-0 flex-1">{children}</div>
  </div>
);

export default DnsConfigEditor;
