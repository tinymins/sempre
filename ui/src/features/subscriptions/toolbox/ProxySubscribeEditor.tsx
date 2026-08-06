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
	  useMemo,
	  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import "@/lib/monaco";
import type { CustomNode, LinuxNetworkInventory, SubscriptionConfigurationContext, SubscriptionEditorConfig, SubscriptionProfile, SubscriptionSource } from "@/lib/types";
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
	configurationContext: SubscriptionConfigurationContext;
}

const DnsConfigEditorField = ({
  value,
  onChange,
  form,
  defaultValue,
	configurationContext,
}: DnsConfigEditorFieldProps) => {
  const useSystem = Form.useWatch("useSystemDnsConfig", form);

  if (useSystem) {
    return (
		<DnsConfigEditor key="system-default" value={defaultValue} readOnly features={configurationContext.capabilities.features} />
    );
  }

  return (
    <DnsConfigEditor
		key="user-custom"
		value={value}
		onChange={onChange}
		features={configurationContext.capabilities.features}
		nativeTarget={dnsNativeTarget(configurationContext)}
	/>
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
	configurationContext: SubscriptionConfigurationContext;
  onSave: (profile: SubscriptionProfile) => Promise<void> | void;
  schedule: { interval: string; autoRestart: boolean };
  onScheduleSave: (change: { interval?: string; auto_restart?: boolean }) => Promise<void> | void;
  diagnostics: ReactNode;
}

const BASE_TABS = [
  { label: "basic", value: "basic" },
  { label: "subscribeUrl", value: "subscribeUrl" },
];

const AUTOSAVE_DELAY = 800;

type SaveFeedback = {
  state: "idle" | "waiting" | "saving" | "saved" | "error";
  message?: string;
};

function dnsNativeTarget(context: SubscriptionConfigurationContext) {
	if (!context.target || !context.capabilities.features.includes("dns.native")) return undefined;
	if (context.target.core === "mihomo") return { key: "mihomo", label: "Mihomo" };
	if (context.target.core === "sing-box") {
		const key = context.target.compiler_target.version === "11" ? "sing_box_v11" : "sing_box_v12";
		return { key, label: context.target.compiler_target.version === "11" ? "sing-box 1.11" : "sing-box 1.12+" };
	}
	return { key: context.target.core, label: context.target.core };
}

function profileFormValues(profile: SubscriptionProfile, configurationContext: SubscriptionConfigurationContext): FormFieldValues {
	const transparent = profile.transparent_proxy ?? {
			mode: "tun-router" as const,
			capture_host: false,
			lan_interfaces: [],
			route_exclusions: [],
			interface_mode: "all" as const,
			interfaces: [],
			auto_exclude_local_routes: true,
			auto_exclude_vpn_routes: true,
			tun: {
				interface_name: "sempre-tun",
			},
			tproxy: { listen_port: 7893, dns_listen_port: 1053 },
			ebpf: { wan_interface: "auto", auto_config_kernel_parameter: false },
		};
	const localProxy = profile.local_proxy ?? { socks_port: 1080, http_port: 1081, username: "sempre", password: "" };
	const managementAPI = profile.management_api ?? { external_controller: "0.0.0.0:9090", secret: "", allow_origins: [], allow_private_network: false };
	const targetCore = configurationContext.target?.core;
	const features = new Set(configurationContext.capabilities.features);
	const transparentMode = (
		transparent.mode === "tun-router" && features.has("transparent.tun") ||
		transparent.mode === "tproxy" && features.has("transparent.tproxy") ||
		transparent.mode === "ebpf-router" && features.has("transparent.ebpf") ||
		transparent.mode === "disabled"
	) ? transparent.mode : "disabled";
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
		advancedConfig: JSON.stringify(targetCore ? profile.core_overrides?.[targetCore] ?? {} : {}, null, 2),
    selectedCustomNodeIds: profile.custom_node_ids ?? [],
		transparentMode,
		tunInterfaceName: transparent.tun.interface_name,
		tunAddress: transparent.tun.address ?? "",
		tunRouteExclusions: transparent.route_exclusions.join("\n"),
		tunInterfaceMode: transparent.interface_mode ?? "all",
		tunInterfaces: transparent.interfaces ?? [],
		tunAutoExcludeLocal: transparent.auto_exclude_local_routes,
		tunAutoExcludeVPN: transparent.auto_exclude_vpn_routes,
		tproxyPort: transparent.tproxy.listen_port,
		tproxyDNSPort: transparent.tproxy.dns_listen_port,
		tproxyCaptureHost: transparent.capture_host,
		tproxyLANInterfaces: transparent.lan_interfaces,
		ebpfWANInterface: transparent.ebpf.wan_interface,
		ebpfAutoConfigKernel: transparent.ebpf.auto_config_kernel_parameter,
		localProxySOCKSPort: localProxy.socks_port,
		localProxyHTTPPort: localProxy.http_port,
		localProxyUsername: localProxy.username,
		localProxyPassword: localProxy.password,
		managementAPIController: managementAPI.external_controller ?? "0.0.0.0:9090",
		managementAPISecret: managementAPI.secret ?? "",
		managementAPIUI: managementAPI.external_ui ?? "",
		managementAPIOrigins: managementAPI.allow_origins,
		managementAPIPrivateNetwork: managementAPI.allow_private_network,
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
	configurationContext,
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
		const features = useMemo(() => new Set(configurationContext.capabilities.features), [configurationContext.capabilities.features]);
		const supportsTransparent = features.has("transparent.tun") || features.has("transparent.tproxy") || features.has("transparent.ebpf");
		const supportsLocalProxy = features.has("inbound.local_proxy");
		const supportsManagement = features.has("management.external_api");
		const supportsDNS = configurationContext.capabilities.features.some((feature) => feature.startsWith("dns."));
		const runtimeVisible = supportsLocalProxy || supportsTransparent || supportsManagement;
		const availableTabs = useMemo(() => [
				...BASE_TABS,
				...(features.has("routing.rule_providers") ? [{ label: "ruleList", value: "ruleList" }] : []),
				...(features.has("routing.selector") || features.has("routing.url_test") ? [{ label: "group", value: "group" }] : []),
				...(features.has("routing.rules") ? [{ label: "customRules", value: "customConfig" }] : []),
				...(configurationContext.target && features.has("native_override") ? [{ label: "advancedConfig", value: "advancedConfig" }] : []),
				...(supportsDNS ? [{ label: "dnsConfig", value: "dnsConfig" }] : []),
				...(features.has("private_access") ? [{ label: "privateAccessConfig", value: "privateAccessConfig" }] : []),
				...(runtimeVisible ? [{ label: "runtime", value: "runtime" }] : []),
				...(configurationContext.capabilities.protocols.length > 0 ? [{ label: "servers", value: "servers" }] : []),
				{ label: "diagnostics", value: "diagnostics" },
			], [configurationContext.capabilities.protocols.length, configurationContext.target, features, runtimeVisible, supportsDNS]);
		const [form] = Form.useForm(profileFormValues(profile, configurationContext));
    const manualServers = Form.useWatch("servers", form) as string | undefined;
		const transparentMode = Form.useWatch("transparentMode", form) as string | undefined;
		const tunInterfaceMode = Form.useWatch("tunInterfaceMode", form) as string | undefined;

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
		const visibleActiveTab = availableTabs.some((tab) => tab.value === activeTab) ? activeTab : "basic";

    const localizedTabs = availableTabs.map((tab) => ({
      ...tab,
      label: (
        <span className={`text-sm ${tab.value === visibleActiveTab ? "font-medium" : "font-normal"}`}>
          {t(`proxy.tabs.${tab.label}`)}
        </span>
      ),
    }));

    const buildCandidate = async (): Promise<SubscriptionProfile> => {
      const values = await form.validateFields();
			const fields = ["ruleList", "group", "customConfig", "dnsConfig", "privateAccessConfig", "servers", ...(configurationContext.target ? ["advancedConfig"] : [])];
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
			if (configurationContext.target && (!advancedConfig || Array.isArray(advancedConfig) || typeof advancedConfig !== "object")) {
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
			const currentOverrides = { ...(profileRef.current.core_overrides ?? {}) };
			if (configurationContext.target) {
				currentOverrides[configurationContext.target.core] = advancedConfig as Record<string, unknown>;
			}
			return {
        ...profileRef.current,
        remark: values.remark || "",
        log_level: values.logLevel ?? "info",
        sources: [...sources, ...rawSourcesRef.current],
        custom_node_ids: values.selectedCustomNodeIds ?? [],
			core_overrides: currentOverrides,
			local_proxy: {
				socks_port: values.localProxySOCKSPort ?? 1080,
				http_port: values.localProxyHTTPPort ?? 1081,
				username: values.localProxyUsername || "sempre",
				password: values.localProxyPassword || profileRef.current.local_proxy?.password || "",
			},
			transparent_proxy: {
				mode: values.transparentMode ?? "tun-router",
				capture_host: values.tproxyCaptureHost ?? false,
				lan_interfaces: values.tproxyLANInterfaces ?? [],
				route_exclusions: String(values.tunRouteExclusions || "").split(/[\n,]/).map((value) => value.trim()).filter(Boolean),
				interface_mode: values.tunInterfaceMode ?? "all",
				interfaces: values.tunInterfaces ?? [],
				auto_exclude_local_routes: values.tunAutoExcludeLocal ?? true,
				auto_exclude_vpn_routes: values.tunAutoExcludeVPN ?? true,
				tun: {
					interface_name: values.tunInterfaceName || "sempre-tun",
					address: values.tunAddress?.trim() || undefined,
				},
				tproxy: {
					listen_port: values.tproxyPort ?? 7893,
					dns_listen_port: values.tproxyDNSPort ?? 1053,
				},
				ebpf: {
					wan_interface: values.ebpfWANInterface || "auto",
					auto_config_kernel_parameter: values.ebpfAutoConfigKernel ?? false,
				},
			},
			management_api: {
				external_controller: values.managementAPIController?.trim() || undefined,
				secret: values.managementAPISecret || undefined,
				external_ui: values.managementAPIUI?.trim() || undefined,
				allow_origins: values.managementAPIOrigins ?? [],
				allow_private_network: values.managementAPIPrivateNetwork ?? false,
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
              activeKey={visibleActiveTab}
              onChange={(key) => setActiveTab(key)}
              items={localizedTabs.map((tab) => ({
                key: tab.value,
                label: tab.label,
              }))}
            />
          </div>
          <SaveStatus profile={profileFeedback} schedule={scheduleFeedback} />
					{configurationContext.target && configurationContext.running && configurationContext.target.core !== configurationContext.running.core ? (
						<Alert
							type="warning"
							showIcon
							message={t("proxy.form.coreTransition", { target: configurationContext.target.core, running: configurationContext.running.core })}
						/>
					) : null}

          <Form form={form} layout="vertical" onValuesChange={queueAutosave}>
            {/* 基础信息 */}
            <div style={{ display: visibleActiveTab === "basic" ? "block" : "none" }}>
              <Form.Item label={t("proxy.form.remark")} name="remark">
                <TextArea
                  rows={3}
                  placeholder={t("proxy.form.remarkPlaceholder")}
                />
              </Form.Item>
              {features.has("logging.level") ? <Form.Item
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
              </Form.Item> : null}
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
                display: visibleActiveTab === "subscribeUrl" ? "block" : "none",
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
                <div key={tab} className={visibleActiveTab === tab ? "" : "hidden"}>
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

            <div className={visibleActiveTab === "advancedConfig" ? "" : "hidden"}>
              <Form.Item
                label={t("proxy.form.advancedConfigLabel")}
                name="advancedConfig"
              >
                <JsoncEditor placeholder={t("proxy.form.advancedConfigPlaceholder")} />
              </Form.Item>
            </div>

            {/* DNS 配置 */}
            <div className={visibleActiveTab === "dnsConfig" ? "" : "hidden"}>
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
								configurationContext={configurationContext}
                />
              </Form.Item>
            </div>

            {/* 内网访问配置 */}
            <div
              className={visibleActiveTab === "privateAccessConfig" ? "" : "hidden"}
            >
              <Form.Item
                label={t("proxy.form.privateAccessConfigLabel")}
                name="privateAccessConfig"
              >
                <PrivateAccessEditor />
              </Form.Item>
            </div>

			<div className={visibleActiveTab === "runtime" ? "space-y-5" : "hidden"}>
				{supportsLocalProxy ? <section className="space-y-4">
					<div className="grid gap-4 md:grid-cols-2">
						<Form.Item label={t("proxy.form.localProxySOCKSPort")} name="localProxySOCKSPort">
							<InputNumber min={1} max={65535} className="w-full" />
						</Form.Item>
						<Form.Item label={t("proxy.form.localProxyHTTPPort")} name="localProxyHTTPPort">
							<InputNumber min={1} max={65535} className="w-full" />
						</Form.Item>
						<Form.Item label={t("proxy.form.localProxyUsername")} name="localProxyUsername">
							<Input autoComplete="username" />
						</Form.Item>
						<Form.Item label={t("proxy.form.localProxyPassword")} name="localProxyPassword">
							<Password autoComplete="new-password" />
						</Form.Item>
					</div>
				</section> : null}
				{supportsTransparent ? <section className="space-y-4">
					<Form.Item label={t("proxy.form.transparentMode")} name="transparentMode">
						<Select
							options={[
								...(features.has("transparent.tun") ? [{ value: "tun-router", label: t("proxy.form.transparentModeTun") }] : []),
								...(features.has("transparent.tproxy") ? [{ value: "tproxy", label: t("proxy.form.transparentModeTProxy") }] : []),
								...(features.has("transparent.ebpf") ? [{ value: "ebpf-router", label: t("proxy.form.transparentModeEBPF") }] : []),
								{ value: "disabled", label: t("proxy.form.transparentModeDisabled") },
							]}
							onChange={(value) => {
								if ((value === "tproxy" || value === "ebpf-router") && (form.getFieldValue("tproxyLANInterfaces") as string[] | undefined)?.length === 0 && networkInventory?.recommended_lan_interfaces.length) {
									form.setFieldValue("tproxyLANInterfaces", networkInventory.recommended_lan_interfaces);
								}
								if (value === "ebpf-router" && !form.getFieldValue("ebpfWANInterface")) {
									form.setFieldValue("ebpfWANInterface", networkInventory?.default_interface || "auto");
								}
							}}
						/>
					</Form.Item>
					{transparentMode === "tun-router" && features.has("transparent.tun") ? (
						<>
							<div className="grid gap-4 md:grid-cols-2">
								<Form.Item label={t("proxy.form.tunInterface")} name="tunInterfaceName">
									<Input />
								</Form.Item>
								{features.has("transparent.tun.address") ? <Form.Item label={t("proxy.form.tunAddress")} name="tunAddress">
									<Input placeholder={t("proxy.form.tunAddressAuto")} />
								</Form.Item> : null}
							</div>
							{features.has("transparent.interface_policy") ? (
								<div className="grid gap-4 md:grid-cols-2">
									<Form.Item label={t("proxy.form.tunInterfacePolicy")} name="tunInterfaceMode">
										<Select options={[
											{ value: "all", label: t("proxy.form.tunInterfaceAll") },
											{ value: "include", label: t("proxy.form.tunInterfaceInclude") },
											{ value: "exclude", label: t("proxy.form.tunInterfaceExclude") },
										]} />
									</Form.Item>
									{tunInterfaceMode !== "all" ? <Form.Item label={t("proxy.form.tunInterfaces")} name="tunInterfaces">
										<Select mode="tags" options={(networkInventory?.interfaces ?? []).map((item) => ({ value: item.name, label: item.name }))} />
									</Form.Item> : null}
								</div>
							) : null}
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
					{transparentMode === "tproxy" && features.has("transparent.tproxy") ? (
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
					{transparentMode === "ebpf-router" && features.has("transparent.ebpf") ? (
						<>
							<div className="grid gap-4 md:grid-cols-2">
								<Form.Item label={t("proxy.form.ebpfWANInterface")} name="ebpfWANInterface">
									<Select options={[
										{ value: "auto", label: t("proxy.form.ebpfWANAuto") },
										...(networkInventory?.interfaces ?? []).filter((item) => item.up).map((item) => ({ value: item.name, label: item.name })),
									]} />
								</Form.Item>
								<Form.Item label={t("proxy.form.ebpfLANInterfaces")} name="tproxyLANInterfaces">
									<Select mode="tags" showSearch options={(networkInventory?.interfaces ?? []).filter((item) => item.up).map((item) => ({ value: item.name, label: item.name }))} />
								</Form.Item>
							</div>
							<Form.Item label={t("proxy.form.ebpfAutoConfigKernel")} name="ebpfAutoConfigKernel" valuePropName="checked">
								<Switch />
							</Form.Item>
						</>
					) : null}
				</section> : null}

				{supportsManagement ? <section className="space-y-4 border-t border-[var(--border)] pt-5">
					<Alert type="warning" showIcon message={t("proxy.form.managementAPISecurityWarning")} />
					<div className="grid gap-4 md:grid-cols-2">
						<Form.Item label={t("proxy.form.managementAPIController")} name="managementAPIController">
							<Input />
						</Form.Item>
						<Form.Item label={t("proxy.form.managementAPISecret")} name="managementAPISecret">
							<Password autoComplete="new-password" />
						</Form.Item>
					</div>
					<Form.Item label={t("proxy.form.managementAPIUI")} name="managementAPIUI">
						<Input />
					</Form.Item>
					<Form.Item label={t("proxy.form.managementAPIOrigins")} name="managementAPIOrigins">
						<Select mode="tags" />
					</Form.Item>
					<Form.Item label={t("proxy.form.managementAPIPrivateNetwork")} name="managementAPIPrivateNetwork" valuePropName="checked">
						<Switch />
					</Form.Item>
				</section> : null}
			</div>

            {/* 额外服务器 */}
            <div className={visibleActiveTab === "servers" ? "" : "hidden"}>
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
          {visibleActiveTab === "diagnostics" ? <div>{diagnostics}</div> : null}
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
