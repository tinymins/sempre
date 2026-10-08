import { Form } from "@acme/components";
import { sourceText, type EditorSource } from "@acme/subscription-editor";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import type { SubscriptionProfile, SubscriptionSource } from "@/lib/types";
import { randomUuid } from "@/lib/randomUuid";

import { AUTOSAVE_DELAY, type Props, type SaveFeedback, profileFormValues } from "./ProxySubscribeModel";

export function useProxySubscribeEditor({
  profile,
  defaults,
  customNodes,
	networkInventory,
	configurationContext,
  onSave,
  onSaveStateChange,
  schedule,
  onScheduleSave,
  diagnostics,
	sourceDebug = true,
}: Props) {
    const { t } = useTranslation();
    const [scheduleInterval, setScheduleInterval] = useState(schedule.interval);
    const [autoRestart, setAutoRestart] = useState(schedule.autoRestart);
    const [profileFeedback, setProfileFeedback] = useState<SaveFeedback>({ state: "idle" });
    const [profileDirty, setProfileDirty] = useState(false);
    const [scheduleFeedback, setScheduleFeedback] = useState<SaveFeedback>({ state: "idle" });
		const features = useMemo(() => new Set(configurationContext.capabilities.features), [configurationContext.capabilities.features]);
		const supportsTransparent = features.has("transparent.tun") || features.has("transparent.tproxy") || features.has("transparent.ebpf");
		const supportsLocalProxy = features.has("inbound.local_proxy");
		const supportsManagement = features.has("management.external_api");
		const savedTransparentMode = profile.transparent_proxy?.mode;
		const unsupportedTransparentMode = savedTransparentMode && savedTransparentMode !== "disabled" && !(
			savedTransparentMode === "tun-router" && features.has("transparent.tun") ||
			savedTransparentMode === "tproxy" && features.has("transparent.tproxy") ||
			savedTransparentMode === "ebpf-router" && features.has("transparent.ebpf")
		);
		const runtimeVisible = supportsLocalProxy || supportsTransparent || supportsManagement || Boolean(unsupportedTransparentMode);
		const [form] = Form.useForm(profileFormValues(profile));
		const transparentMode = Form.useWatch("transparentMode", form) as string | undefined;
		const tunInterfaceMode = Form.useWatch("tunInterfaceMode", form) as string | undefined;

    const mountedRef = useRef(true);
    const profileRef = useRef(profile);
    const onSaveRef = useRef(onSave);
    const onScheduleSaveRef = useRef(onScheduleSave);
    const buildCandidateRef = useRef<(() => Promise<SubscriptionProfile>) | undefined>(undefined);
    const runAutosaveRef = useRef<() => Promise<void>>(async () => undefined);
    const runScheduleSaveRef = useRef<() => Promise<void>>(async () => undefined);
    const profileTimerRef = useRef<number | undefined>(undefined);
    const profileRevisionRef = useRef(0);
    const profileDirtyRef = useRef(false);
    const profileSaveRequestedRef = useRef(false);
    const profileSaveInFlightRef = useRef(false);
    const scheduleTimerRef = useRef<number | undefined>(undefined);
    const schedulePatchRef = useRef<{ interval?: string; auto_restart?: boolean }>({});
    const scheduleSaveInFlightRef = useRef(false);

    useEffect(() => {
      profileRef.current = profile;
      onSaveRef.current = onSave;
      onScheduleSaveRef.current = onScheduleSave;
    }, [profile, onSave, onScheduleSave]);

    useEffect(() => {
      onSaveStateChange?.({
        profileID: profile.id,
        dirty: profileDirty,
        saving: profileFeedback.state === "saving",
      });
    }, [onSaveStateChange, profile.id, profileDirty, profileFeedback.state]);

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

    const buildCandidate = async (): Promise<SubscriptionProfile> => {
      const values = form.getFieldsValue();
      const cleanedItems = ((values.subscribeItems as EditorSource[]) || [])
        .filter((item: EditorSource) => item.type === "raw" || sourceText(item).trim());
      const sources: SubscriptionSource[] = cleanedItems.map((item: EditorSource) => item.type === "raw" ? {
        ...profileRef.current.sources.find(source => source.id === item.id),
        id: item.id || randomUuid(),
        type: "raw",
        content: item.content,
        enabled: item.enabled,
        prefix: item.prefix || undefined,
        remark: item.remark || undefined,
      } : ({
        id: item.id || randomUuid(),
        type: item.type,
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
        sources,
        custom_node_ids: values.selectedCustomNodeIds ?? [],
			local_proxy: {
				socks_port: values.localProxySOCKSPort ?? 20580,
				http_port: values.localProxyHTTPPort ?? 20581,
				username: values.localProxyUsername ?? profileRef.current.local_proxy?.username ?? "sempre",
				password: values.localProxyPassword ?? profileRef.current.local_proxy?.password ?? "",
			},
			transparent_proxy: {
				mode: values.transparentMode ?? "tun-router",
					capture_host: profileRef.current.transparent_proxy?.capture_host ?? false,
					lan_interfaces: profileRef.current.transparent_proxy?.lan_interfaces ?? [],
				route_exclusions: String(values.tunRouteExclusions || "").split(/[\n,]/).map((value) => value.trim()).filter(Boolean),
				interface_mode: values.tunInterfaceMode ?? "all",
				interfaces: values.tunInterfaces ?? [],
				auto_exclude_local_routes: values.tunAutoExcludeLocal ?? true,
				auto_exclude_vpn_routes: values.tunAutoExcludeVPN ?? true,
				tun: {
					interface_name: String(values.tunInterfaceName ?? ""),
					address: values.tunAddress?.trim() || undefined,
				},
				tproxy: {
					listen_port: values.tproxyPort ?? 20582,
					dns_listen_port: values.tproxyDNSPort ?? 20553,
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
          profileDirtyRef.current = false;
          setProfileDirty(false);
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
      profileDirtyRef.current = true;
      profileSaveRequestedRef.current = true;
      window.clearTimeout(profileTimerRef.current);
      if (mountedRef.current) {
        setProfileDirty(true);
        setProfileFeedback({ state: "waiting" });
      }
      profileTimerRef.current = window.setTimeout(() => void runAutosaveRef.current(), AUTOSAVE_DELAY);
    }, []);

    const saveNow = useCallback(() => {
      if (!profileDirtyRef.current || profileSaveInFlightRef.current) return;
      profileSaveRequestedRef.current = true;
      void runAutosaveRef.current();
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

  return {
    t,
    profileFeedback,
    scheduleFeedback,
    configurationContext,
    form,
    queueAutosave,
    saveNow,
    features,
    scheduleInterval,
    setScheduleInterval,
    queueScheduleSave,
    autoRestart,
    setAutoRestart,
    defaults,
    supportsLocalProxy,
    supportsTransparent,
    runtimeVisible,
    supportsManagement,
    transparentMode,
    tunInterfaceMode,
    networkInventory,
    customNodes,
		diagnostics,
		sourceDebug,
  };
}
