import { Form } from "@acme/components";
import type { SubscribeItem } from "@acme/types";
import { parse as parseJsonc } from "jsonc-parser";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import type { SubscriptionProfile, SubscriptionSource } from "@/lib/types";

import { AUTOSAVE_DELAY, BASE_TABS, type Props, type SaveFeedback, isValidJsonc, profileFormValues } from "./ProxySubscribeModel";

export function useProxySubscribeEditor({
  profile,
  defaults,
  customNodes,
	networkInventory,
	configurationContext,
  onSave,
  schedule,
  onScheduleSave,
  diagnostics,
}: Props) {
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
					interface_name: String(values.tunInterfaceName ?? "").trim(),
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

  return {
    t,
    setActiveTab,
    visibleActiveTab,
    localizedTabs,
    profileFeedback,
    scheduleFeedback,
    configurationContext,
    form,
    queueAutosave,
    features,
    scheduleInterval,
    setScheduleInterval,
    queueScheduleSave,
    autoRestart,
    setAutoRestart,
    rawSources,
    rawSourcesRef,
    setRawSources,
    defaults,
    CONFIG_FIELDS,
    supportsLocalProxy,
    supportsTransparent,
    supportsManagement,
    transparentMode,
    tunInterfaceMode,
    networkInventory,
    customNodes,
    manualServerCount,
    openManualServersEditor,
    diagnostics,
    manualServersEditorOpen,
    setManualServersEditorOpen,
    saveManualServers,
    manualServersDraft,
    setManualServersDraft,
    manualServersError,
  };
}
