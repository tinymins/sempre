import type { FormFieldValues } from "@acme/components";
import type { SubscribeItem } from "@acme/types";
import { parse as parseJsonc, type ParseError } from "jsonc-parser";
import type { ReactNode } from "react";
import type { CustomNode, LinuxNetworkInventory, SubscriptionConfigurationContext, SubscriptionEditorConfig, SubscriptionProfile } from "@/lib/types";


export interface Props {
  profile: SubscriptionProfile;
  defaults: SubscriptionEditorConfig & { by_core?: Record<string, SubscriptionEditorConfig> };
  customNodes: CustomNode[];
	networkInventory?: LinuxNetworkInventory;
	configurationContext: SubscriptionConfigurationContext;
  onSave: (profile: SubscriptionProfile) => Promise<void> | void;
  onSaveStateChange?: (state: ProxySubscribeSaveState) => void;
  schedule: { interval: string; autoRestart: boolean };
  onScheduleSave: (change: { interval?: string; auto_restart?: boolean }) => Promise<void> | void;
  showAutoRestart?: boolean;
  diagnostics: ReactNode;
  sourceDebug?: boolean;
  readOnly?: boolean;
}

export type ProxySubscribeSaveState = {
	profileID: string;
	dirty: boolean;
	saving: boolean;
};

export const BASE_TABS = [
  { label: "basic", value: "basic" },
  { label: "subscribeUrl", value: "subscribeUrl" },
];

export const AUTOSAVE_DELAY = 800;

export type SaveFeedback = {
  state: "idle" | "waiting" | "saving" | "saved" | "error";
  message?: string;
};

export function recommendedEditorDefaults(defaults: Props["defaults"], configurationContext: SubscriptionConfigurationContext): SubscriptionEditorConfig {
	return defaults.by_core?.[configurationContext.target?.core ?? ""] ?? defaults;
}

export function profileFormValues(profile: SubscriptionProfile, configurationContext: SubscriptionConfigurationContext): FormFieldValues {
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
			tproxy: { listen_port: 7893, dns_listen_port: 20553 },
			ebpf: { wan_interface: "auto", auto_config_kernel_parameter: false },
		};
	const localProxy = profile.local_proxy ?? { socks_port: 1080, http_port: 1081, username: "sempre", password: "" };
	const managementAPI = profile.management_api ?? { external_controller: "0.0.0.0:9090", secret: "", allow_origins: [], allow_private_network: false };
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
    subscribeItems: items,
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

export function isValidJsonc(value: string) {
  const errors: ParseError[] = [];
  parseJsonc(value, errors, { allowTrailingComma: true });
  return errors.length === 0;
}
