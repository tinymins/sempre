import { Input, InputNumber, Switch, Tabs } from "@acme/components";
import type { DnsConfig, DnsSharedConfig } from "@acme/types";
import Editor, { type Monaco } from "@monaco-editor/react";
import { parse as parseJsonc } from "jsonc-parser";
import type React from "react";
import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import "@/lib/monaco";

type TopTab = "shared" | "singbox" | "singboxV12" | "clash" | "clashMeta";

/** 默认 DNS shared 配置（需要与服务端 DEFAULT_DNS_SHARED 保持一致） */
const CLIENT_DEFAULT_SHARED: Required<DnsSharedConfig> = {
  localDns: "127.0.0.1",
  localDnsPort: 53,
  fakeipIpv4Range: "198.18.0.0/15",
  fakeipIpv6Range: "fc00::/18",
  fakeipEnabled: true,
  fakeipTtl: 300,
  dnsListenPort: 1053,
  tproxyPort: 7893,
  rejectHttps: true,
  cnDomainLocalDns: true,
  clashApiPort: 9999,
  clashApiSecret: "123456",
  clashApiUiPath: "/etc/sb/ui",
};

interface DnsConfigEditorProps {
  value?: string;
  onChange?: (value: string) => void;
  readOnly?: boolean;
}

/** 安全解析 JSONC → DnsConfig（新结构 {shared, overrides}） */
const parseDnsConfig = (jsonc: string | undefined): DnsConfig => {
  if (!jsonc) return {};
  try {
    const parsed = parseJsonc(jsonc);
    return parsed && typeof parsed === "object" ? (parsed as DnsConfig) : {};
  } catch {
    return {};
  }
};

/** 序列化 DnsConfig → JSON 字符串（仅保留有意义的数据） */
const serializeDnsConfig = (config: DnsConfig): string => {
  const result: DnsConfig = {};

  // shared: 仅保留与默认值不同的字段
  if (config.shared) {
    const diff: Record<string, unknown> = {};
    for (const [key, val] of Object.entries(config.shared)) {
      const defaultVal =
        CLIENT_DEFAULT_SHARED[key as keyof Required<DnsSharedConfig>];
      if (JSON.stringify(val) !== JSON.stringify(defaultVal)) {
        diff[key] = val;
      }
    }
    if (Object.keys(diff).length > 0) {
      result.shared = diff as DnsSharedConfig;
    }
  }

  // overrides: 仅保留非空的
  if (config.overrides) {
    const overrides: DnsConfig["overrides"] = {};
    if (
      config.overrides.singbox &&
      Object.keys(config.overrides.singbox).length > 0
    ) {
      overrides.singbox = config.overrides.singbox;
    }
    if (
      config.overrides.singboxV12 &&
      Object.keys(config.overrides.singboxV12).length > 0
    ) {
      overrides.singboxV12 = config.overrides.singboxV12;
    }
    if (
      config.overrides.clash &&
      Object.keys(config.overrides.clash).length > 0
    ) {
      overrides.clash = config.overrides.clash;
    }
    if (
      config.overrides.clashMeta &&
      Object.keys(config.overrides.clashMeta).length > 0
    ) {
      overrides.clashMeta = config.overrides.clashMeta;
    }
    if (Object.keys(overrides).length > 0) {
      result.overrides = overrides;
    }
  }

  if (Object.keys(result).length === 0) return "";
  return JSON.stringify(result, null, 2);
};

const DnsConfigEditor = ({
  value,
  onChange,
  readOnly,
}: DnsConfigEditorProps) => {
  const { t } = useTranslation();
  const [tab, setTab] = useState<TopTab>("shared");

  const parsed = useMemo(() => parseDnsConfig(value), [value]);
  const mergedShared = useMemo(
    () => ({ ...CLIENT_DEFAULT_SHARED, ...(parsed.shared ?? {}) }),
    [parsed],
  );

  // shared 表单字段变化
  const handleSharedChange = useCallback(
    (field: keyof DnsSharedConfig, val: unknown) => {
      if (readOnly) return;
      const current = parseDnsConfig(value);
      const updatedShared = {
        ...CLIENT_DEFAULT_SHARED,
        ...(current.shared ?? {}),
        [field]: val,
      };
      onChange?.(serializeDnsConfig({ ...current, shared: updatedShared }));
    },
    [value, onChange, readOnly],
  );

  // override JSONC 变化
  const handleOverrideChange = useCallback(
    (
      key: "singbox" | "singboxV12" | "clash" | "clashMeta",
      jsonStr: string,
    ) => {
      if (readOnly) return;
      const current = parseDnsConfig(value);
      let parsed: Record<string, unknown> | undefined;
      if (jsonStr.trim()) {
        try {
          parsed = parseJsonc(jsonStr) ?? undefined;
        } catch {
          return;
        }
      }
      const overrides = { ...(current.overrides ?? {}) };
      if (parsed && Object.keys(parsed).length > 0) {
        overrides[key] = parsed;
      } else {
        delete overrides[key];
      }
      onChange?.(serializeDnsConfig({ ...current, overrides }));
    },
    [value, onChange, readOnly],
  );

  const overrideKey = tab !== "shared" ? tab : null;
  const overrideValue = useMemo(() => {
    if (!overrideKey) return "";
    const data = parsed.overrides?.[overrideKey];
    return data ? JSON.stringify(data, null, 2) : "";
  }, [parsed, overrideKey]);

  const overridePlaceholders: Record<string, string> = {
    singbox: `// sing-box v1.11 ${t("proxy.form.dnsNativeHint")}`,
    singboxV12: `// sing-box v1.12+ ${t("proxy.form.dnsNativeHint")}`,
    clash: `// Clash ${t("proxy.form.dnsNativeHint")}`,
    clashMeta: `// Clash Meta ${t("proxy.form.dnsNativeHint")}`,
  };

  return (
    <div className="space-y-3">
      {!readOnly && (
        <Tabs
          type="segment"
          size="small"
          activeKey={tab}
          onChange={(v) => setTab(v as TopTab)}
          items={[
            { key: "shared", label: t("proxy.form.dnsTabShared") },
            { key: "clash", label: "Clash" },
            { key: "clashMeta", label: "Clash Meta" },
            { key: "singbox", label: "Sing-box v1.11" },
            { key: "singboxV12", label: "Sing-box v1.12+" },
          ].map((item) => ({
            ...item,
            label: <span className={tab === item.key ? "font-medium" : "font-normal"}>{item.label}</span>,
          }))}
        />
      )}

      {(tab === "shared" || readOnly) && (
        <SharedForm
          merged={mergedShared}
          readOnly={readOnly}
          onFieldChange={handleSharedChange}
        />
      )}

      {overrideKey && !readOnly && (
        <div className="space-y-2">
          <span className="text-xs text-slate-500">
            {t("proxy.form.dnsOverrideHint")}
          </span>
          <JsoncEditor
            value={overrideValue}
            onChange={(val) => handleOverrideChange(overrideKey, val)}
            placeholder={overridePlaceholders[overrideKey]}
          />
        </div>
      )}
    </div>
  );
};

// ═══════════════════════════════════════════════════
// Shared Form
// ═══════════════════════════════════════════════════

interface SharedFormProps {
  merged: Required<DnsSharedConfig>;
  readOnly?: boolean;
  onFieldChange: (field: keyof DnsSharedConfig, val: unknown) => void;
}

const SharedForm = ({ merged, readOnly, onFieldChange }: SharedFormProps) => {
  const { t } = useTranslation();
  const disabled = readOnly ?? false;

  return (
    <div className="space-y-3">
      {/* ── DNS 基础 ── */}
      <SectionTitle title="DNS" />
      <div className="grid grid-cols-2 gap-x-4 gap-y-2">
        <FieldRow label={t("proxy.form.dnsLocalDns")}>
          <Input
            size="small"
            value={merged.localDns}
            disabled={disabled}
            onChange={(e) => onFieldChange("localDns", e.target.value)}
          />
        </FieldRow>
        <FieldRow label={t("proxy.form.dnsLocalDnsPort")}>
          <InputNumber
            size="small"
            className="w-full"
            min={1}
            max={65535}
            value={merged.localDnsPort}
            disabled={disabled}
            onChange={(v) => onFieldChange("localDnsPort", v)}
          />
        </FieldRow>
        <FieldRow label={t("proxy.form.dnsDnsListenPort")}>
          <InputNumber
            size="small"
            className="w-full"
            min={1}
            max={65535}
            value={merged.dnsListenPort}
            disabled={disabled}
            onChange={(v) => onFieldChange("dnsListenPort", v)}
          />
        </FieldRow>
        <FieldRow label={t("proxy.form.dnsTproxyPort")}>
          <InputNumber
            size="small"
            className="w-full"
            min={1}
            max={65535}
            value={merged.tproxyPort}
            disabled={disabled}
            onChange={(v) => onFieldChange("tproxyPort", v)}
          />
        </FieldRow>
      </div>

      {/* ── FakeIP ── */}
      <SectionTitle title="FakeIP" />
      <div className="grid grid-cols-2 gap-x-4 gap-y-2">
        <FieldRow label={t("proxy.form.dnsFakeipEnabled")}>
          <Switch
            size="small"
            checked={merged.fakeipEnabled}
            disabled={disabled}
            onChange={(v) => onFieldChange("fakeipEnabled", v)}
          />
        </FieldRow>
        <FieldRow label={t("proxy.form.dnsFakeipTtl")}>
          <InputNumber
            size="small"
            className="w-full"
            min={0}
            value={merged.fakeipTtl}
            disabled={disabled}
            onChange={(v) => onFieldChange("fakeipTtl", v)}
          />
        </FieldRow>
        <FieldRow label={t("proxy.form.dnsFakeipIpv4Range")}>
          <Input
            size="small"
            value={merged.fakeipIpv4Range}
            disabled={disabled}
            onChange={(e) => onFieldChange("fakeipIpv4Range", e.target.value)}
          />
        </FieldRow>
        <FieldRow label={t("proxy.form.dnsFakeipIpv6Range")}>
          <Input
            size="small"
            value={merged.fakeipIpv6Range}
            disabled={disabled}
            onChange={(e) => onFieldChange("fakeipIpv6Range", e.target.value)}
          />
        </FieldRow>
      </div>

      {/* ── DNS 规则 ── */}
      <SectionTitle title={t("proxy.form.dnsDnsRules")} />
      <div className="grid grid-cols-2 gap-x-4 gap-y-2">
        <FieldRow label={t("proxy.form.dnsRejectHttps")}>
          <Switch
            size="small"
            checked={merged.rejectHttps}
            disabled={disabled}
            onChange={(v) => onFieldChange("rejectHttps", v)}
          />
        </FieldRow>
        <FieldRow label={t("proxy.form.dnsCnDomainLocalDns")}>
          <Switch
            size="small"
            checked={merged.cnDomainLocalDns}
            disabled={disabled}
            onChange={(v) => onFieldChange("cnDomainLocalDns", v)}
          />
        </FieldRow>
      </div>

      {/* ── Clash API ── */}
      <SectionTitle title="Clash API" />
      <div className="grid grid-cols-2 gap-x-4 gap-y-2">
        <FieldRow label={t("proxy.form.dnsClashApiPort")}>
          <InputNumber
            size="small"
            className="w-full"
            min={1}
            max={65535}
            value={merged.clashApiPort}
            disabled={disabled}
            onChange={(v) => onFieldChange("clashApiPort", v)}
          />
        </FieldRow>
        <FieldRow label={t("proxy.form.dnsClashApiSecret")}>
          <Input
            size="small"
            value={merged.clashApiSecret}
            disabled={disabled}
            onChange={(e) => onFieldChange("clashApiSecret", e.target.value)}
          />
        </FieldRow>
        <FieldRow label={t("proxy.form.dnsClashApiUiPath")} span2>
          <Input
            size="small"
            value={merged.clashApiUiPath}
            disabled={disabled}
            onChange={(e) => onFieldChange("clashApiUiPath", e.target.value)}
          />
        </FieldRow>
      </div>
    </div>
  );
};

// ═══════════════════════════════════════════════════
// Reusable JSONC Editor
// ═══════════════════════════════════════════════════

interface JsoncEditorProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}

const JsoncEditor = ({ value, onChange, placeholder }: JsoncEditorProps) => (
  <div className="border rounded overflow-hidden border-gray-300 dark:border-gray-600">
    <Editor
      height={300}
      language="json"
      value={value || (placeholder ?? "")}
      theme="vs-dark"
      onChange={(val: string | undefined) => onChange(val || "")}
      options={{
        automaticLayout: true,
        selectOnLineNumbers: true,
        fontSize: 14,
        fontFamily: "Menlo, Monaco, 'Courier New', monospace",
        wordWrap: "on",
        renderControlCharacters: true,
        renderWhitespace: "all",
        scrollBeyondLastLine: false,
        minimap: { enabled: false },
        tabSize: 2,
      }}
      beforeMount={(monaco: Monaco) => {
        monaco.languages.json.jsonDefaults.setDiagnosticsOptions({
          validate: true,
          allowComments: true,
          trailingCommas: "ignore",
        });
        monaco.editor.defineTheme("vs-dark", {
          base: "vs-dark",
          inherit: true,
          rules: [],
          colors: { "editor.background": "#141414" },
        });
      }}
    />
  </div>
);

// ═══════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════

/** 小节标题 */
const SectionTitle = ({ title }: { title: string }) => (
  <span className="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wide pt-1">
    {title}
  </span>
);

/** 一行表单字段 */
const FieldRow = ({
  label,
  children,
  span2,
}: {
  label: string;
  children: React.ReactNode;
  span2?: boolean;
}) => (
  <div className={`flex items-center gap-2 ${span2 ? "col-span-2" : ""}`}>
    <span className="text-xs whitespace-nowrap min-w-[120px] text-right text-gray-600 dark:text-gray-400">
      {label}
    </span>
    <div className="flex-1">{children}</div>
  </div>
);

export default DnsConfigEditor;
