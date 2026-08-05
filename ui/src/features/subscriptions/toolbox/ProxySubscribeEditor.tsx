import {
	Alert,
  Button,
  Checkbox,
  type FormFieldValues,
  Form,
  Input,
	InputNumber,
  Modal,
	Password,
  Select,
	Switch,
  Tabs,
  Tag,
  TextArea,
} from "@acme/components";
import type { SubscribeItem } from "@acme/types";
import Editor, { type Monaco } from "@monaco-editor/react";
import { parse as parseJsonc, type ParseError } from "jsonc-parser";
import {
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import "@/lib/monaco";
import type { CustomNode, LinuxNetworkInventory, SubscriptionEditorConfig, SubscriptionProfile, SubscriptionSource } from "@/lib/types";
import DnsConfigEditor from "./DnsConfigEditor";
import PrivateAccessEditor from "./PrivateAccessEditor";
import SubscribeItemsEditor from "./SubscribeItemsEditor";
import TagListEditor from "./TagListEditor";

// JSONC 编辑器组件，支持 // 和 /* */ 注释
interface JsoncEditorProps {
  value?: string;
  onChange?: (value: string) => void;
  placeholder?: string;
  readOnly?: boolean;
}

const JsoncEditor = ({ value, onChange, readOnly }: JsoncEditorProps) => {
  return (
    <div
      className={`border rounded overflow-hidden ${
        readOnly
          ? "border-gray-500 dark:border-gray-500 opacity-60"
          : "border-gray-300 dark:border-gray-600"
      }`}
    >
      <Editor
        height="calc(100vh - 280px)"
        language="json"
        value={value || ""}
        theme="vs-dark"
        onChange={(val: string | undefined) => {
          if (!readOnly) onChange?.(val || "");
        }}
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
          readOnly: readOnly ?? false,
        }}
        beforeMount={(monaco: Monaco) => {
          // 配置 JSON 语言允许注释和尾随逗号
          monaco.languages.json.jsonDefaults.setDiagnosticsOptions({
            validate: true,
            allowComments: true,
            trailingCommas: "ignore",
          });
          monaco.editor.defineTheme("vs-dark", {
            base: "vs-dark",
            inherit: true,
            rules: [],
            colors: {
              "editor.background": "#141414",
            },
          });
        }}
      />
    </div>
  );
};

/**
 * 配置字段编辑器：根据 useSystem checkbox 状态切换只读/可编辑模式。
 * 勾选时显示系统默认值（只读），取消勾选显示用户自定义值（可编辑）。
 */
interface ConfigFieldEditorProps {
  value?: string;
  onChange?: (value: string) => void;
  form: ReturnType<typeof Form.useForm>[0];
  useSystemField: string;
  defaultValue: string;
  placeholder?: string;
}

const ConfigFieldEditor = ({
  value,
  onChange,
  form,
  useSystemField,
  defaultValue,
  placeholder,
}: ConfigFieldEditorProps) => {
  const useSystem = Form.useWatch(useSystemField, form);

  if (useSystem) {
    // 独立的只读编辑器，不连接 form 的 onChange，确保表单值不被覆盖
    return (
      <JsoncEditor
        key="system-default"
        value={defaultValue}
        readOnly
        placeholder={placeholder}
      />
    );
  }

  // 可编辑编辑器，连接 form
  return (
    <JsoncEditor
      key="user-custom"
      value={value}
      onChange={onChange}
      placeholder={placeholder}
    />
  );
};

/**
 * DNS 配置字段编辑器：根据 useSystemDnsConfig 切换只读/可编辑模式。
 * 使用 DnsConfigEditor 组件而非纯 JSONC 编辑器。
 */
interface DnsConfigEditorFieldProps {
  value?: string;
  onChange?: (value: string) => void;
  form: ReturnType<typeof Form.useForm>[0];
  defaultValue: string;
}

const DnsConfigEditorField = ({
  value,
  onChange,
  form,
  defaultValue,
}: DnsConfigEditorFieldProps) => {
  const useSystem = Form.useWatch("useSystemDnsConfig", form);

  if (useSystem) {
    return (
      <DnsConfigEditor key="system-default" value={defaultValue} readOnly />
    );
  }

  return (
    <DnsConfigEditor key="user-custom" value={value} onChange={onChange} />
  );
};

/**
 * Node filter field: bridges JSON string ↔ string[] for TagListEditor.
 * Uses useSystemFilter to toggle between system default (read-only) and user custom.
 */
interface NodeFilterFieldProps {
  form: ReturnType<typeof Form.useForm>[0];
  defaultValue: string;
}

const NodeFilterField = ({ form, defaultValue }: NodeFilterFieldProps) => {
  const { t } = useTranslation();
  const useSystem = Form.useWatch("useSystemFilter", form);

  const parseFilterJson = (json: string): string[] => {
    try {
      const parsed = parseJsonc(json);
      return Array.isArray(parsed)
        ? parsed.filter((s): s is string => typeof s === "string")
        : [];
    } catch {
      return [];
    }
  };

  if (useSystem) {
    return <TagListEditor value={parseFilterJson(defaultValue)} readOnly />;
  }

  return (
    <Form.Item name="filter" noStyle>
      <NodeFilterTagAdapter
        placeholder={t("proxy.form.nodeFilterAddPlaceholder")}
      />
    </Form.Item>
  );
};

/**
 * Adapter: converts Form's string value ↔ TagListEditor's string[] value.
 */
interface NodeFilterTagAdapterProps {
  value?: string;
  onChange?: (value: string) => void;
  placeholder?: string;
}

const NodeFilterTagAdapter = ({
  value,
  onChange,
  placeholder,
}: NodeFilterTagAdapterProps) => {
  const parseFilterJson = (json: string): string[] => {
    try {
      const parsed = parseJsonc(json);
      return Array.isArray(parsed)
        ? parsed.filter((s): s is string => typeof s === "string")
        : [];
    } catch {
      return [];
    }
  };

  const tags = parseFilterJson(value ?? "");

  const handleChange = (newTags: string[]) => {
    onChange?.(JSON.stringify(newTags));
  };

  return (
    <TagListEditor
      value={tags}
      onChange={handleChange}
      placeholder={placeholder}
    />
  );
};

interface Props {
  profile: SubscriptionProfile;
  defaults: SubscriptionEditorConfig;
  customNodes: CustomNode[];
	networkInventory?: LinuxNetworkInventory;
  onSave: (profile: SubscriptionProfile) => Promise<void> | void;
  schedule: { interval: string; autoRestart: boolean };
  onScheduleSave: (change: { interval?: string; auto_restart?: boolean }) => Promise<void> | void;
  diagnostics: ReactNode;
}

const TABS = [
  { label: "basic", value: "basic" },
  { label: "subscribeUrl", value: "subscribeUrl" },
  { label: "ruleList", value: "ruleList" },
  { label: "group", value: "group" },
  { label: "customRules", value: "customConfig" },
  { label: "advancedConfig", value: "advancedConfig" },
  { label: "dnsConfig", value: "dnsConfig" },
  { label: "privateAccessConfig", value: "privateAccessConfig" },
	{ label: "runtime", value: "runtime" },
  { label: "servers", value: "servers" },
  { label: "diagnostics", value: "diagnostics" },
];

const AUTOSAVE_DELAY = 800;

type SaveFeedback = {
  state: "idle" | "waiting" | "saving" | "saved" | "error";
  message?: string;
};

function profileFormValues(profile: SubscriptionProfile): FormFieldValues {
	const transparent = profile.transparent_proxy ?? {
		mode: "tun-router" as const,
		tun: {
			interface_name: "sing-box",
			route_exclude_address: [],
			auto_exclude_local_routes: true,
			auto_exclude_vpn_routes: true,
		},
		tproxy: { listen_port: 7893, dns_listen_port: 1053, capture_host: false, lan_interfaces: [] },
	};
	const clashAPI = profile.clash_api ?? { enabled: false, allow_origins: [], allow_private_network: false };
  const items: SubscribeItem[] = profile.sources
    .filter((source) => source.type === "url")
    .map((source) => ({
      id: source.id,
      enabled: source.enabled,
      url: source.url ?? "",
      prefix: source.prefix ?? "",
      remark: source.remark ?? "",
      cacheTtlMinutes: source.cache_ttl_minutes,
      fetchUa: source.user_agent || undefined,
      fetchMode: source.fetch_mode ?? "auto",
    }));
  return {
    remark: profile.remark ?? "",
    logLevel: profile.log_level ?? "info",
    subscribeItems:
      items.length > 0
        ? items
        : [
            {
              enabled: true,
              url: "",
              prefix: "",
              remark: "",
              fetchMode: "auto",
            },
          ],
    ruleList: profile.editor.rule_list ?? "",
    useSystemRuleList: profile.use_system_rules,
    group: profile.editor.group ?? "",
    useSystemGroup: profile.use_system_groups,
    filter: profile.editor.filter ?? "",
    useSystemFilter: profile.use_system_filters,
    customConfig: profile.editor.custom_config ?? "",
    useSystemCustomConfig: profile.use_system_custom_config,
    dnsConfig: profile.editor.dns_config ?? "",
    useSystemDnsConfig: profile.use_system_dns,
    privateAccessConfig: profile.editor.private_access_config ?? "",
    servers: profile.editor.servers || "[]",
    advancedConfig: JSON.stringify(profile.custom_config ?? {}, null, 2),
    selectedCustomNodeIds: profile.custom_node_ids ?? [],
		transparentMode: transparent.mode,
		tunInterfaceName: transparent.tun.interface_name,
		tunAddress: transparent.tun.address ?? "",
		tunRouteExclusions: transparent.tun.route_exclude_address.join("\n"),
		tunAutoExcludeLocal: transparent.tun.auto_exclude_local_routes,
		tunAutoExcludeVPN: transparent.tun.auto_exclude_vpn_routes,
		tproxyPort: transparent.tproxy.listen_port,
		tproxyDNSPort: transparent.tproxy.dns_listen_port,
		tproxyCaptureHost: transparent.tproxy.capture_host,
		tproxyLANInterfaces: transparent.tproxy.lan_interfaces,
		clashAPIEnabled: clashAPI.enabled,
		clashAPIController: clashAPI.external_controller ?? "0.0.0.0:9090",
		clashAPISecret: clashAPI.secret ?? "",
		clashAPIUI: clashAPI.external_ui ?? "",
		clashAPIOrigins: clashAPI.allow_origins,
		clashAPIPrivateNetwork: clashAPI.allow_private_network,
  };
}

function isValidJsonc(value: string) {
  const errors: ParseError[] = [];
  parseJsonc(value, errors, { allowTrailingComma: true });
  return errors.length === 0;
}

const ProxySubscribeEditor = ({
  profile,
  defaults,
  customNodes,
	networkInventory,
  onSave,
  schedule,
  onScheduleSave,
  diagnostics,
}: Props) => {
    const { t } = useTranslation();
    const [activeTab, setActiveTab] = useState("basic");
    const [manualServersEditorOpen, setManualServersEditorOpen] =
      useState(false);
    const [manualServersDraft, setManualServersDraft] = useState("");
    const [manualServersError, setManualServersError] = useState("");
    const [rawSources, setRawSources] = useState<SubscriptionSource[]>(() =>
      profile.sources.filter((source) => source.type === "raw"),
    );
    const [scheduleInterval, setScheduleInterval] = useState(schedule.interval);
    const [autoRestart, setAutoRestart] = useState(schedule.autoRestart);
    const [profileFeedback, setProfileFeedback] = useState<SaveFeedback>({ state: "idle" });
    const [scheduleFeedback, setScheduleFeedback] = useState<SaveFeedback>({ state: "idle" });
    const [form] = Form.useForm(profileFormValues(profile));
    const manualServers = Form.useWatch("servers", form) as string | undefined;
		const transparentMode = Form.useWatch("transparentMode", form) as string | undefined;
		const clashAPIEnabled = Form.useWatch("clashAPIEnabled", form) as boolean | undefined;

    const mountedRef = useRef(true);
    const profileRef = useRef(profile);
    const rawSourcesRef = useRef(rawSources);
    const onSaveRef = useRef(onSave);
    const onScheduleSaveRef = useRef(onScheduleSave);
    const buildCandidateRef = useRef<(() => Promise<SubscriptionProfile>) | undefined>(undefined);
    const runAutosaveRef = useRef<() => Promise<void>>(async () => undefined);
    const runScheduleSaveRef = useRef<() => Promise<void>>(async () => undefined);
    const profileTimerRef = useRef<number | undefined>(undefined);
    const profileRevisionRef = useRef(0);
    const profileSaveRequestedRef = useRef(false);
    const profileSaveInFlightRef = useRef(false);
    const scheduleTimerRef = useRef<number | undefined>(undefined);
    const schedulePatchRef = useRef<{ interval?: string; auto_restart?: boolean }>({});
    const scheduleSaveInFlightRef = useRef(false);

    useEffect(() => {
      profileRef.current = profile;
      rawSourcesRef.current = rawSources;
      onSaveRef.current = onSave;
      onScheduleSaveRef.current = onScheduleSave;
    }, [profile, rawSources, onSave, onScheduleSave]);

    useEffect(() => {
      mountedRef.current = true;
      return () => {
        mountedRef.current = false;
        window.clearTimeout(profileTimerRef.current);
        window.clearTimeout(scheduleTimerRef.current);
        if (profileSaveRequestedRef.current) void runAutosaveRef.current();
        if (Object.keys(schedulePatchRef.current).length > 0) void runScheduleSaveRef.current();
      };
    }, [profile.id]);

    // 获取 tabs 的本地化标签
    const localizedTabs = TABS.map((tab) => ({
      ...tab,
      label: (
        <span className={`text-sm ${tab.value === activeTab ? "font-medium" : "font-normal"}`}>
          {t(`proxy.tabs.${tab.label}`)}
        </span>
      ),
    }));

    const buildCandidate = async (): Promise<SubscriptionProfile> => {
      const values = await form.validateFields();
      const fields = ["ruleList", "group", "customConfig", "dnsConfig", "privateAccessConfig", "servers", "advancedConfig"];
      for (const field of fields) {
        if (values[field] && !isValidJsonc(values[field])) {
          throw new Error(`${t(`proxy.tabs.${field === "customConfig" ? "customRules" : field}`)}: ${t("proxy.form.jsonFormatError")}`);
        }
      }
      const advancedConfig = parseJsonc(values.advancedConfig || "{}") as unknown;
      const customRules = parseJsonc(values.customConfig || "[]") as unknown;
      if (!Array.isArray(customRules) || customRules.some((rule) => typeof rule !== "string")) {
        throw new Error(t("proxy.form.customRulesArrayError"));
      }
      if (!advancedConfig || Array.isArray(advancedConfig) || typeof advancedConfig !== "object") {
        throw new Error(t("proxy.form.advancedConfigObjectError"));
      }
      const cleanedItems = ((values.subscribeItems as SubscribeItem[]) || [])
        .filter((item: SubscribeItem) => item.url?.trim());
      const sources: SubscriptionSource[] = cleanedItems.map((item: SubscribeItem) => ({
        id: item.id || crypto.randomUUID(),
        type: "url",
        enabled: item.enabled,
        url: item.url.trim(),
        prefix: item.prefix || undefined,
        remark: item.remark || undefined,
        user_agent: item.fetchUa || "clash.meta",
        fetch_mode: item.fetchMode ?? "auto",
        cache_ttl_minutes: item.cacheTtlMinutes,
      }));
      return {
        ...profileRef.current,
        remark: values.remark || "",
        log_level: values.logLevel ?? "info",
        sources: [...sources, ...rawSourcesRef.current],
        custom_node_ids: values.selectedCustomNodeIds ?? [],
        custom_config: advancedConfig as Record<string, unknown>,
			transparent_proxy: {
				mode: values.transparentMode ?? "tun-router",
				tun: {
					interface_name: values.tunInterfaceName || "sing-box",
					address: values.tunAddress?.trim() || undefined,
					route_exclude_address: String(values.tunRouteExclusions || "").split(/[\n,]/).map((value) => value.trim()).filter(Boolean),
					auto_exclude_local_routes: values.tunAutoExcludeLocal ?? true,
					auto_exclude_vpn_routes: values.tunAutoExcludeVPN ?? true,
				},
				tproxy: {
					listen_port: values.tproxyPort ?? 7893,
					dns_listen_port: values.tproxyDNSPort ?? 1053,
					capture_host: values.tproxyCaptureHost ?? false,
					lan_interfaces: values.tproxyLANInterfaces ?? [],
				},
			},
			clash_api: {
				enabled: values.clashAPIEnabled ?? false,
				external_controller: values.clashAPIController?.trim() || undefined,
				secret: values.clashAPISecret || undefined,
				external_ui: values.clashAPIUI?.trim() || undefined,
				allow_origins: values.clashAPIOrigins ?? [],
				allow_private_network: values.clashAPIPrivateNetwork ?? false,
			},
        use_system_rules: values.useSystemRuleList ?? true,
        use_system_groups: values.useSystemGroup ?? true,
        use_system_filters: values.useSystemFilter ?? true,
        use_system_custom_config: values.useSystemCustomConfig ?? true,
        use_system_dns: values.useSystemDnsConfig ?? true,
        editor: {
          rule_list: values.ruleList || "",
          group: values.group || "",
          filter: values.filter || "",
          custom_config: values.customConfig || "",
          dns_config: values.dnsConfig || "",
          private_access_config: values.privateAccessConfig || "",
          servers: values.servers || "[]",
        },
      };
    };
    useEffect(() => {
      buildCandidateRef.current = buildCandidate;
    });

    const runAutosave = useCallback(async () => {
      window.clearTimeout(profileTimerRef.current);
      if (profileSaveInFlightRef.current) return;
      profileSaveRequestedRef.current = false;
      let candidate: SubscriptionProfile;
      try {
        candidate = await buildCandidateRef.current!();
      } catch (error) {
        if (mountedRef.current) setProfileFeedback({ state: "error", message: error instanceof Error ? error.message : String(error) });
        return;
      }
      const revision = profileRevisionRef.current;
      profileSaveInFlightRef.current = true;
      if (mountedRef.current) setProfileFeedback({ state: "saving" });
      try {
        await onSaveRef.current(candidate);
        if (mountedRef.current && revision === profileRevisionRef.current && !profileSaveRequestedRef.current) {
          setProfileFeedback({ state: "saved" });
        }
      } catch (error) {
        if (mountedRef.current) setProfileFeedback({ state: "error", message: error instanceof Error ? error.message : String(error) });
      } finally {
        profileSaveInFlightRef.current = false;
        if (profileSaveRequestedRef.current || revision !== profileRevisionRef.current) {
          profileSaveRequestedRef.current = true;
          void runAutosaveRef.current();
        }
      }
    }, []);
    useEffect(() => {
      runAutosaveRef.current = runAutosave;
    }, [runAutosave]);

    const queueAutosave = useCallback(() => {
      profileRevisionRef.current += 1;
      profileSaveRequestedRef.current = true;
      window.clearTimeout(profileTimerRef.current);
      if (mountedRef.current) setProfileFeedback({ state: "waiting" });
      profileTimerRef.current = window.setTimeout(() => void runAutosaveRef.current(), AUTOSAVE_DELAY);
    }, []);

    const runScheduleSave = useCallback(async () => {
      window.clearTimeout(scheduleTimerRef.current);
      if (scheduleSaveInFlightRef.current || Object.keys(schedulePatchRef.current).length === 0) return;
      const patch = schedulePatchRef.current;
      schedulePatchRef.current = {};
      scheduleSaveInFlightRef.current = true;
      if (mountedRef.current) setScheduleFeedback({ state: "saving" });
      try {
        await onScheduleSaveRef.current(patch);
        if (mountedRef.current && Object.keys(schedulePatchRef.current).length === 0) setScheduleFeedback({ state: "saved" });
      } catch (error) {
        if (mountedRef.current) setScheduleFeedback({ state: "error", message: error instanceof Error ? error.message : String(error) });
      } finally {
        scheduleSaveInFlightRef.current = false;
        if (Object.keys(schedulePatchRef.current).length > 0) void runScheduleSaveRef.current();
      }
    }, []);
    useEffect(() => {
      runScheduleSaveRef.current = runScheduleSave;
    }, [runScheduleSave]);

    const queueScheduleSave = useCallback((patch: { interval?: string; auto_restart?: boolean }, immediate = false) => {
      schedulePatchRef.current = { ...schedulePatchRef.current, ...patch };
      window.clearTimeout(scheduleTimerRef.current);
      if (mountedRef.current) setScheduleFeedback({ state: "waiting" });
      if (immediate) {
        void runScheduleSaveRef.current();
      } else {
        scheduleTimerRef.current = window.setTimeout(() => void runScheduleSaveRef.current(), AUTOSAVE_DELAY);
      }
    }, []);

    const manualServerCount = (() => {
      try {
        const parsed = parseJsonc(manualServers || "[]");
        return Array.isArray(parsed) ? parsed.length : 0;
      } catch {
        return 0;
      }
    })();

    const openManualServersEditor = () => {
      setManualServersDraft(manualServers || JSON.stringify([], null, 2));
      setManualServersError("");
      setManualServersEditorOpen(true);
    };

    const saveManualServers = (): undefined => {
      if (isValidJsonc(manualServersDraft)) {
        form.setFieldValue("servers", manualServersDraft);
        setManualServersEditorOpen(false);
        setManualServersError("");
        queueAutosave();
      } else {
        setManualServersError(t("proxy.form.jsonFormatError"));
      }
      return undefined;
    };

    // 配置字段定义（用于统一渲染 useSystem checkbox + editor）
    type ConfigField = "ruleList" | "group" | "customConfig";
    const CONFIG_FIELDS: {
      field: ConfigField;
      useSystemField: string;
      tab: string;
      labelKey: string;
      placeholderKey: string;
    }[] = [
      {
        field: "ruleList",
        useSystemField: "useSystemRuleList",
        tab: "ruleList",
        labelKey: "proxy.form.ruleListLabel",
        placeholderKey: "proxy.form.ruleListPlaceholder",
      },
      {
        field: "group",
        useSystemField: "useSystemGroup",
        tab: "group",
        labelKey: "proxy.form.groupLabel",
        placeholderKey: "proxy.form.groupPlaceholder",
      },
      {
        field: "customConfig",
        useSystemField: "useSystemCustomConfig",
        tab: "customConfig",
        labelKey: "proxy.form.customConfigLabel",
        placeholderKey: "proxy.form.customConfigPlaceholder",
      },
    ];

    return (
      <div className="min-h-0 rounded-lg border border-black/[0.08] bg-white/50 p-4 dark:border-white/[0.08] dark:bg-white/[0.02]">
          <div className="mb-3 shrink-0 overflow-x-auto pb-1">
            <Tabs
              className="min-w-[920px]"
              type="segment"
              activeKey={activeTab}
              onChange={(key) => setActiveTab(key)}
              items={localizedTabs.map((tab) => ({
                key: tab.value,
                label: tab.label,
              }))}
            />
          </div>
          <SaveStatus profile={profileFeedback} schedule={scheduleFeedback} />

          <Form form={form} layout="vertical" onValuesChange={queueAutosave}>
            {/* 基础信息 */}
            <div style={{ display: activeTab === "basic" ? "block" : "none" }}>
              <Form.Item label={t("proxy.form.remark")} name="remark">
                <TextArea
                  rows={3}
                  placeholder={t("proxy.form.remarkPlaceholder")}
                />
              </Form.Item>
              <Form.Item
                label={t("proxy.form.logLevel")}
                name="logLevel"
                tooltip={t("proxy.form.logLevelTip")}
              >
                <Select
                  options={[
                    {
                      value: "off",
                      label: t("proxy.form.logLevelOff"),
                    },
                    {
                      value: "error",
                      label: t("proxy.form.logLevelError"),
                    },
                    {
                      value: "warn",
                      label: t("proxy.form.logLevelWarn"),
                    },
                    {
                      value: "info",
                      label: t("proxy.form.logLevelInfo"),
                    },
                    {
                      value: "debug",
                      label: t("proxy.form.logLevelDebug"),
                    },
                  ]}
                />
              </Form.Item>
              <div className="grid gap-4 border-t border-gray-200 pt-4 dark:border-gray-700 md:grid-cols-2">
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>{t("proxy.form.updateSchedule")}</span>
                  <Input
                    value={scheduleInterval}
                    onChange={(event) => {
                      const interval = event.target.value;
                      setScheduleInterval(interval);
                      queueScheduleSave({ interval });
                    }}
                  />
                </label>
                <label className="flex min-h-9 items-center gap-2 self-end rounded-md border border-[var(--border)] px-3 text-sm">
                  <Checkbox
                    checked={autoRestart}
                    onChange={(event) => {
                      const value = event.target.checked;
                      setAutoRestart(value);
                      queueScheduleSave({ auto_restart: value }, true);
                    }}
                  />
                  <span>{t("proxy.form.restartAfterScheduledUpdates")}</span>
                </label>
              </div>
            </div>

            {/* 订阅源 */}
            <div
              style={{
                display: activeTab === "subscribeUrl" ? "block" : "none",
              }}
            >
              <Form.Item
                label={t("proxy.form.subscribeUrlLabel")}
                name="subscribeItems"
              >
                <SubscribeItemsEditor />
              </Form.Item>

              {rawSources.map((source, index) => (
                <div key={source.id} className="mt-3 rounded-lg border border-gray-200 p-3 dark:border-gray-700">
                  <div className="mb-2 flex items-center gap-2">
                    <Tag>RAW</Tag>
                    <input
                      className="min-w-0 flex-1 bg-transparent text-sm outline-none"
                      value={source.remark ?? ""}
                      placeholder={t("proxy.form.subscribeItemRemark")}
                    onChange={(event) => {
                      const next = rawSourcesRef.current.map((item, position) => position === index ? { ...item, remark: event.target.value } : item);
                      rawSourcesRef.current = next;
                      setRawSources(next);
                      queueAutosave();
                    }}
                  />
                    <Button variant="text" size="small" danger onClick={() => {
                      const next = rawSourcesRef.current.filter((_, position) => position !== index);
                      rawSourcesRef.current = next;
                      setRawSources(next);
                      queueAutosave();
                    }}>
                      {t("proxy.actions.delete")}
                    </Button>
                  </div>
                  <TextArea
                    rows={8}
                    value={source.content ?? ""}
                    placeholder="proxies:"
                    onChange={(event) => {
                      const next = rawSourcesRef.current.map((item, position) => position === index ? { ...item, content: event.target.value } : item);
                      rawSourcesRef.current = next;
                      setRawSources(next);
                      queueAutosave();
                    }}
                  />
                </div>
              ))}
              <Button
                className="!mt-3"
                variant="dashed"
                block
                onClick={() => {
                  const next: SubscriptionSource[] = [...rawSourcesRef.current, { id: crypto.randomUUID(), type: "raw", enabled: true, content: "", remark: "" }];
                  rawSourcesRef.current = next;
                  setRawSources(next);
                  queueAutosave();
                }}
              >
                {t("proxy.form.addRawSource")}
              </Button>

              {/* 节点过滤器 */}
              <div className="mt-4 pt-4 border-t border-gray-200 dark:border-zinc-700">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-sm font-medium">
                    {t("proxy.form.nodeFilterLabel")}
                  </span>
                  <Form.Item
                    name="useSystemFilter"
                    valuePropName="checked"
                    noStyle
                  >
                    <Checkbox>{t("proxy.form.useSystemConfig")}</Checkbox>
                  </Form.Item>
                </div>
                <NodeFilterField
                  form={form}
                  defaultValue={defaults.filter || "[]"}
                />
              </div>
            </div>

            {/* 规则列表 / 分组 / 过滤器 / 自定义配置 */}
            {CONFIG_FIELDS.map(
              ({ field, useSystemField, tab, labelKey, placeholderKey }) => (
                <div key={tab} className={activeTab === tab ? "" : "hidden"}>
                  <div className="flex items-center justify-between mb-2">
                    <span>{t(labelKey)}</span>
                    <Form.Item
                      name={useSystemField}
                      valuePropName="checked"
                      noStyle
                    >
                      <Checkbox>{t("proxy.form.useSystemConfig")}</Checkbox>
                    </Form.Item>
                  </div>
                  <Form.Item
                    name={field}
                    dependencies={[useSystemField]}
                    noStyle
                  >
                    <ConfigFieldEditor
                      form={form}
                      useSystemField={useSystemField}
                      defaultValue={field === "ruleList" ? defaults.rule_list : field === "group" ? defaults.group : defaults.custom_config}
                      placeholder={t(placeholderKey)}
                    />
                  </Form.Item>
                </div>
              ),
            )}

            <div className={activeTab === "advancedConfig" ? "" : "hidden"}>
              <Form.Item
                label={t("proxy.form.advancedConfigLabel")}
                name="advancedConfig"
              >
                <JsoncEditor placeholder={t("proxy.form.advancedConfigPlaceholder")} />
              </Form.Item>
            </div>

            {/* DNS 配置 */}
            <div className={activeTab === "dnsConfig" ? "" : "hidden"}>
              <div className="flex items-center justify-between mb-2">
                <span>{t("proxy.form.dnsConfigLabel")}</span>
                <Form.Item
                  name="useSystemDnsConfig"
                  valuePropName="checked"
                  noStyle
                >
                  <Checkbox>{t("proxy.form.useSystemConfig")}</Checkbox>
                </Form.Item>
              </div>
              <Form.Item
                name="dnsConfig"
                dependencies={["useSystemDnsConfig"]}
                noStyle
              >
                <DnsConfigEditorField
                  form={form}
                  defaultValue={defaults.dns_config}
                />
              </Form.Item>
            </div>

            {/* 内网访问配置 */}
            <div
              className={activeTab === "privateAccessConfig" ? "" : "hidden"}
            >
              <Form.Item
                label={t("proxy.form.privateAccessConfigLabel")}
                name="privateAccessConfig"
              >
                <PrivateAccessEditor />
              </Form.Item>
            </div>

			<div className={activeTab === "runtime" ? "space-y-5" : "hidden"}>
				<section className="space-y-4">
					<Form.Item label={t("proxy.form.transparentMode")} name="transparentMode">
						<Select
							options={[
								{ value: "tun-router", label: t("proxy.form.transparentModeTun") },
								{ value: "tproxy", label: t("proxy.form.transparentModeTProxy") },
								{ value: "disabled", label: t("proxy.form.transparentModeDisabled") },
							]}
							onChange={(value) => {
								if (value === "tproxy" && (form.getFieldValue("tproxyLANInterfaces") as string[] | undefined)?.length === 0 && networkInventory?.recommended_lan_interfaces.length) {
									form.setFieldValue("tproxyLANInterfaces", networkInventory.recommended_lan_interfaces);
								}
							}}
						/>
					</Form.Item>
					{transparentMode === "tun-router" ? (
						<>
							<div className="grid gap-4 md:grid-cols-2">
								<Form.Item label={t("proxy.form.tunInterface")} name="tunInterfaceName">
									<Input />
								</Form.Item>
								<Form.Item label={t("proxy.form.tunAddress")} name="tunAddress">
									<Input placeholder={t("proxy.form.tunAddressAuto")} />
								</Form.Item>
							</div>
							<Form.Item label={t("proxy.form.tunRouteExclusions")} name="tunRouteExclusions">
								<TextArea rows={3} />
							</Form.Item>
							<div className="grid gap-4 md:grid-cols-2">
								<Form.Item label={t("proxy.form.tunAutoExcludeLocal")} name="tunAutoExcludeLocal" valuePropName="checked">
									<Switch />
								</Form.Item>
								<Form.Item label={t("proxy.form.tunAutoExcludeVPN")} name="tunAutoExcludeVPN" valuePropName="checked">
									<Switch />
								</Form.Item>
							</div>
						</>
					) : null}
					{transparentMode === "tproxy" ? (
						<>
							<div className="grid gap-4 md:grid-cols-2">
								<Form.Item label={t("proxy.form.tproxyPort")} name="tproxyPort">
									<InputNumber min={1} max={65535} className="w-full" />
								</Form.Item>
								<Form.Item label={t("proxy.form.tproxyDNSPort")} name="tproxyDNSPort">
									<InputNumber min={1} max={65535} className="w-full" />
								</Form.Item>
							</div>
							<Form.Item label={t("proxy.form.tproxyLANInterfaces")} name="tproxyLANInterfaces">
								<Select
									mode="tags"
									showSearch
									options={(networkInventory?.interfaces ?? []).filter((item) => item.up).map((item) => ({
										value: item.name,
										label: `${item.name} · ${item.kind}${item.default_route ? ` · ${t("proxy.form.defaultRoute")}` : ""}`,
										tagLabel: item.name,
									}))}
								/>
							</Form.Item>
							<Form.Item label={t("proxy.form.tproxyCaptureHost")} name="tproxyCaptureHost" valuePropName="checked">
								<Switch />
							</Form.Item>
						</>
					) : null}
				</section>

				<section className="space-y-4 border-t border-[var(--border)] pt-5">
					<Form.Item label={t("proxy.form.clashAPIEnabled")} name="clashAPIEnabled" valuePropName="checked">
						<Switch />
					</Form.Item>
					{clashAPIEnabled ? (
						<>
							<Alert type="warning" showIcon message={t("proxy.form.clashAPISecurityWarning")} />
							<div className="grid gap-4 md:grid-cols-2">
								<Form.Item label={t("proxy.form.clashAPIController")} name="clashAPIController">
									<Input />
								</Form.Item>
								<Form.Item label={t("proxy.form.clashAPISecret")} name="clashAPISecret">
									<Password autoComplete="new-password" />
								</Form.Item>
							</div>
							<Form.Item label={t("proxy.form.clashAPIUI")} name="clashAPIUI">
								<Input />
							</Form.Item>
							<Form.Item label={t("proxy.form.clashAPIOrigins")} name="clashAPIOrigins">
								<Select mode="tags" />
							</Form.Item>
							<Form.Item label={t("proxy.form.clashAPIPrivateNetwork")} name="clashAPIPrivateNetwork" valuePropName="checked">
								<Switch />
							</Form.Item>
						</>
					) : null}
				</section>
			</div>

            {/* 额外服务器 */}
            <div className={activeTab === "servers" ? "" : "hidden"}>
              <Form.Item
                label={t("proxy.form.assignedCustomNodes")}
                name="selectedCustomNodeIds"
                tooltip={t("proxy.form.assignedCustomNodesTip")}
              >
                <Select
                  mode="multiple"
                  showSearch
                  placeholder={t("proxy.form.assignedCustomNodesPlaceholder")}
                  options={customNodes.map(
                    (node: CustomNode) => ({
                      value: node.id,
                      label: `${node.name} · ${String(node.proxy.type || "")} · ${String(node.proxy.server || "")}:${String(node.proxy.port || "")}`,
                      tagLabel: node.name,
                    }),
                  )}
                />
              </Form.Item>
              <Form.Item name="servers" hidden>
                <TextArea />
              </Form.Item>
              <div className="mt-6 flex items-center gap-2">
                <span className="text-sm text-[var(--text-secondary)]">
                  {t("proxy.form.serversLabel")}
                </span>
                <Tag>{manualServerCount}</Tag>
                <Button
                  variant="link"
                  size="small"
                  onClick={openManualServersEditor}
                >
                  {t("proxy.actions.edit")}
                </Button>
              </div>
            </div>
          </Form>
          {activeTab === "diagnostics" ? <div>{diagnostics}</div> : null}
        <Modal
          title={t("proxy.form.serversLabel")}
          open={manualServersEditorOpen}
          onCancel={() => setManualServersEditorOpen(false)}
          onOk={saveManualServers}
          okText={t("common.save")}
          cancelText={t("common.cancel")}
          width={900}
          destroyOnClose
        >
          <JsoncEditor
            value={manualServersDraft}
            onChange={setManualServersDraft}
            placeholder={t("proxy.form.serversPlaceholder")}
          />
          {manualServersError ? <p role="alert" className="mt-2 text-sm text-red-500">{manualServersError}</p> : null}
        </Modal>
      </div>
    );
};

function SaveStatus({ profile, schedule }: { profile: SaveFeedback; schedule: SaveFeedback }) {
  const { t } = useTranslation();
  const feedback = profile.state === "error" ? profile
    : schedule.state === "error" ? schedule
      : profile.state === "saving" || schedule.state === "saving" ? { state: "saving" as const }
        : profile.state === "waiting" || schedule.state === "waiting" ? { state: "waiting" as const }
          : profile.state === "saved" || schedule.state === "saved" ? { state: "saved" as const }
            : { state: "idle" as const };
  const label = feedback.state === "waiting" ? t("proxy.autosave.waiting")
    : feedback.state === "saving" ? t("proxy.autosave.saving")
      : feedback.state === "saved" ? t("proxy.autosave.saved")
        : feedback.message || "";
  return (
    <div className="mb-4 min-h-6 border-b border-gray-200 pb-3 text-sm dark:border-gray-700">
      {label ? <p role={feedback.state === "error" ? "alert" : "status"} className={feedback.state === "error" ? "break-words text-red-500" : "text-[var(--text-secondary)]"}>{label}</p> : null}
    </div>
  );
}

export default ProxySubscribeEditor;
