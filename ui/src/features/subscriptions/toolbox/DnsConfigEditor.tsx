import { Input, InputNumber, Select, Switch, Tabs } from "@acme/components";
import type { DnsConfig, DnsSharedConfig } from "@acme/types";
import Editor, { type Monaco } from "@monaco-editor/react";
import { parse as parseJsonc } from "jsonc-parser";
import type React from "react";
import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import "@/lib/monaco";

type TopTab = "shared" | "native";

const CLIENT_DEFAULT_SHARED: Required<DnsSharedConfig> = {
  localDns: "local",
  localDnsPort: 53,
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
  preferIpv4: true,
  systemDnsTakeoverEnabled: false,
  systemDnsListenPort: 53,
};

interface DnsConfigEditorProps {
  value?: string;
  onChange?: (value: string) => void;
  readOnly?: boolean;
  nativeTarget?: { key: string; label: string };
  features?: string[];
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
  if (config.modes && Object.keys(config.modes).length > 0) result.modes = config.modes;
  if (config.overrides && Object.keys(config.overrides).length > 0) result.overrides = config.overrides;
  return Object.keys(result).length > 0 ? JSON.stringify(result, null, 2) : "";
};

const DnsConfigEditor = ({ value, onChange, readOnly, nativeTarget, features = [] }: DnsConfigEditorProps) => {
  const { t } = useTranslation();
  const [tab, setTab] = useState<TopTab>("shared");
  const parsed = useMemo(() => parseDnsConfig(value), [value]);
  const mergedShared = useMemo(() => ({ ...CLIENT_DEFAULT_SHARED, ...(parsed.shared ?? {}) }), [parsed]);
  const visibleTab: TopTab = nativeTarget ? tab : "shared";

  const update = useCallback((next: DnsConfig) => onChange?.(serializeDnsConfig(next)), [onChange]);

  const handleSharedChange = useCallback((field: keyof DnsSharedConfig, fieldValue: unknown) => {
    if (readOnly) return;
    const current = parseDnsConfig(value);
    update({ ...current, shared: { ...CLIENT_DEFAULT_SHARED, ...(current.shared ?? {}), [field]: fieldValue } });
  }, [readOnly, update, value]);

  const mode = nativeTarget ? parsed.modes?.[nativeTarget.key] ?? "managed" : "managed";
  const overrideValue = nativeTarget && parsed.overrides?.[nativeTarget.key]
    ? JSON.stringify(parsed.overrides[nativeTarget.key], null, 2)
    : "";

  const handleModeChange = (nextMode: string) => {
    if (!nativeTarget || readOnly) return;
    const current = parseDnsConfig(value);
    update({ ...current, modes: { ...(current.modes ?? {}), [nativeTarget.key]: nextMode as "managed" | "native" } });
  };

  const handleOverrideChange = (json: string) => {
    if (!nativeTarget || readOnly) return;
    const current = parseDnsConfig(value);
    const overrides = { ...(current.overrides ?? {}) };
    if (!json.trim()) {
      delete overrides[nativeTarget.key];
    } else {
      const document = parseJsonc(json) as Record<string, unknown> | undefined;
      if (!document || Array.isArray(document) || typeof document !== "object") return;
      overrides[nativeTarget.key] = document;
    }
    update({ ...current, overrides });
  };

  const tabs = [
    { key: "shared", label: t("proxy.form.dnsTabShared") },
    ...(nativeTarget ? [{ key: "native", label: nativeTarget.label }] : []),
  ];

  return (
    <div className="space-y-3">
      {!readOnly && tabs.length > 1 ? (
        <Tabs
          type="segment"
          size="small"
          activeKey={visibleTab}
          onChange={(key) => setTab(key as TopTab)}
          items={tabs.map((item) => ({ ...item, label: <span className={visibleTab === item.key ? "font-medium" : "font-normal"}>{item.label}</span> }))}
        />
      ) : null}

      {visibleTab === "shared" || readOnly ? (
        <SharedForm merged={mergedShared} readOnly={readOnly} features={features} onFieldChange={handleSharedChange} />
      ) : null}

      {visibleTab === "native" && nativeTarget && !readOnly ? (
        <div className="space-y-3">
          <FieldRow label={t("proxy.form.dnsConfigurationMode")}>
            <Select
              value={mode}
              options={[
                { value: "managed", label: t("proxy.form.dnsModeManaged") },
                { value: "native", label: t("proxy.form.dnsModeNative") },
              ]}
              onChange={handleModeChange}
            />
          </FieldRow>
          {mode === "native" ? <JsoncEditor value={overrideValue} onChange={handleOverrideChange} /> : null}
        </div>
      ) : null}
    </div>
  );
};

interface SharedFormProps {
  merged: Required<DnsSharedConfig>;
  readOnly?: boolean;
  features: string[];
  onFieldChange: (field: keyof DnsSharedConfig, value: unknown) => void;
}

const SharedForm = ({ merged, readOnly, features, onFieldChange }: SharedFormProps) => {
  const { t } = useTranslation();
  const disabled = readOnly ?? false;
  const supported = new Set(features);
  return (
    <div className="space-y-4">
      {supported.has("dns.local_upstream") ? (
        <>
          <SectionTitle title={t("proxy.form.dnsLocalSection")} />
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            <FieldRow label={t("proxy.form.dnsLocalDns")}><Input size="small" value={merged.localDns} disabled={disabled} onChange={(event) => onFieldChange("localDns", event.target.value)} /></FieldRow>
            <FieldRow label={t("proxy.form.dnsLocalDnsPort")}><InputNumber size="small" className="w-full" min={1} max={65535} value={merged.localDnsPort} disabled={disabled} onChange={(next) => onFieldChange("localDnsPort", next)} /></FieldRow>
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
            {supported.has("dns.prefer_ipv4") ? <FieldRow label={t("proxy.form.dnsPreferIpv4")}><Switch size="small" checked={merged.preferIpv4} disabled={disabled} onChange={(next) => onFieldChange("preferIpv4", next)} /></FieldRow> : null}
          </div>
        </>
      ) : null}

      {supported.has("dns.system_takeover") ? (
        <>
          <SectionTitle title={t("proxy.form.dnsSystemSection")} />
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            <FieldRow label={t("proxy.form.dnsSystemTakeover")}><Switch size="small" checked={merged.systemDnsTakeoverEnabled} disabled={disabled} onChange={(next) => onFieldChange("systemDnsTakeoverEnabled", next)} /></FieldRow>
            <FieldRow label={t("proxy.form.dnsSystemListenPort")}><InputNumber size="small" className="w-full" min={1} max={65535} value={merged.systemDnsListenPort} disabled={disabled || !merged.systemDnsTakeoverEnabled} onChange={(next) => onFieldChange("systemDnsListenPort", next)} /></FieldRow>
          </div>
        </>
      ) : null}
    </div>
  );
};

const JsoncEditor = ({ value, onChange }: { value: string; onChange: (value: string) => void }) => (
  <div className="overflow-hidden rounded border border-gray-300 dark:border-gray-600">
    <Editor
      height={300}
      language="json"
      value={value}
      theme="vs-dark"
      onChange={(next) => onChange(next || "")}
      options={{ automaticLayout: true, fontSize: 14, fontFamily: "Menlo, Monaco, 'Courier New', monospace", wordWrap: "on", scrollBeyondLastLine: false, minimap: { enabled: false }, tabSize: 2 }}
      beforeMount={(monaco: Monaco) => {
        monaco.languages.json.jsonDefaults.setDiagnosticsOptions({ validate: true, allowComments: true, trailingCommas: "ignore" });
        monaco.editor.defineTheme("vs-dark", { base: "vs-dark", inherit: true, rules: [], colors: { "editor.background": "#141414" } });
      }}
    />
  </div>
);

const SectionTitle = ({ title }: { title: string }) => <span className="block pt-1 text-xs font-semibold text-gray-500 dark:text-gray-400">{title}</span>;

const FieldRow = ({ label, children, span2 }: { label: string; children: React.ReactNode; span2?: boolean }) => (
  <div className={`flex min-w-0 items-center gap-2 ${span2 ? "md:col-span-2" : ""}`}>
    <span className="min-w-28 shrink-0 text-right text-xs text-gray-600 dark:text-gray-400">{label}</span>
    <div className="min-w-0 flex-1">{children}</div>
  </div>
);

export default DnsConfigEditor;
