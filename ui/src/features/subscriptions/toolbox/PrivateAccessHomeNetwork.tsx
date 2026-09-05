import { Button, Checkbox, PlusOutlined, Tag } from "@acme/components";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { useI18n } from "@/lib/i18n";
import { useOptionalSession } from "@/lib/session";
import type { NetworkSettings, NetworkSettingsResponse, PrivateAccessConnectorStatus, PrivateAccessStatus } from "@/lib/types";

interface Props {
  enabled: boolean;
  networkIds: string[];
  runtime?: PrivateAccessStatus;
  connectorStatus?: PrivateAccessConnectorStatus;
  onChange: (patch: { homeNetworkEnabled?: boolean; homeNetworkIds?: string[] }) => void;
}

export function PrivateAccessHomeNetwork({ enabled, networkIds, runtime, connectorStatus, onChange }: Props) {
  const { t } = useTranslation();
  const { locale } = useI18n();
  const session = useOptionalSession()?.session;
  const queryClient = useQueryClient();
  const zh = locale === "zh-CN";
  const network = useQuery({ queryKey: ["network", "settings"], queryFn: () => api<NetworkSettingsResponse>(session!, "/network/settings"), enabled: Boolean(session) });
  const save = useMutation({
    mutationFn: (settings: NetworkSettings) => api<NetworkSettingsResponse>(session!, "/network/settings", { method: "PUT", body: JSON.stringify(settings) }),
    onSuccess: (result) => queryClient.setQueryData(["network", "settings"], result),
  });
  const mode = connectorStatus?.mode;
  const label = mode === "direct" ? t("proxy.form.privateHomeNetworkDirect") : mode === "wireguard" ? t("proxy.form.privateHomeNetworkWireGuard") : mode === "inactive" ? t("proxy.form.privateHomeNetworkInactive") : mode === "unknown" ? t("proxy.form.privateHomeNetworkUnknown") : t("proxy.form.privateHomeNetworkPending");
  const color = mode === "direct" ? "green" : mode === "wireguard" ? "blue" : "orange";
  const current = network.data?.current;

  function addCurrent() {
    const settings = network.data?.settings;
    if (!settings || !current?.gateway_mac) return;
    const existing = settings.known_networks.find((item) => item.gateway_mac.toLowerCase() === current.gateway_mac?.toLowerCase());
    if (existing) {
      if (!networkIds.includes(existing.id)) onChange({ homeNetworkIds: [...networkIds, existing.id] });
      return;
    }
    const id = crypto.randomUUID();
    const suffix = current.gateway_mac.split(":").slice(-3).join(":");
    save.mutate({
      ...settings,
      automatic_switching: true,
      known_networks: [...settings.known_networks, { id, name: zh ? `家庭网络 ${suffix}` : `Home network ${suffix}`, gateway_mac: current.gateway_mac, disable_proxy: true }],
    }, { onSuccess: () => onChange({ homeNetworkIds: [...networkIds, id] }) });
  }

  return <div className="space-y-3 rounded-md border border-emerald-500/25 bg-emerald-500/5 p-3">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <Checkbox checked={enabled} onChange={(event) => onChange({ homeNetworkEnabled: event.target.checked })}>{t("proxy.form.privateHomeNetworkEnabled")}</Checkbox>
      {enabled ? <div className="flex items-center gap-2 text-xs text-gray-500 dark:text-gray-400"><span>{connectorStatus ? t("proxy.form.privateHomeNetworkApplied") : t("proxy.form.privateHomeNetworkPending")}</span><Tag color={color}>{label}</Tag></div> : null}
    </div>
    {enabled ? <div className="space-y-2">
      {(network.data?.settings.known_networks ?? []).map((item) => <Checkbox key={item.id} checked={networkIds.includes(item.id)} onChange={(event) => onChange({ homeNetworkIds: event.target.checked ? [...networkIds, item.id] : networkIds.filter((id) => id !== item.id) })}>{item.name} · <span className="font-mono text-xs">{item.gateway_mac}</span></Checkbox>)}
      <Button size="small" variant="dashed" disabled={!current?.gateway_mac || save.isPending} onClick={addCurrent}><PlusOutlined />{zh ? "将当前网络加入并设为家庭网络" : "Add current network as home"}</Button>
      {(network.data?.settings.known_networks?.length ?? 0) === 0 ? <p className="text-xs text-gray-500 dark:text-gray-400">{zh ? "请在家中点击上方按钮，Sempre 会自动读取默认网关 MAC。" : "Use the button at home; Sempre will capture the default gateway MAC."}</p> : null}
    </div> : null}
    {enabled && connectorStatus ? <p className="text-xs text-gray-500 dark:text-gray-400">{runtime?.interface || "-"} · {runtime?.interface_addresses.join(", ") || "-"}{connectorStatus.matched_network ? ` · ${connectorStatus.matched_network}` : ""}</p> : null}
    {save.isError ? <p className="text-xs text-red-600">{save.error instanceof Error ? save.error.message : String(save.error)}</p> : null}
  </div>;
}
