import { Checkbox, Input, Tag } from "@acme/components";
import { useTranslation } from "react-i18next";
import type { PrivateAccessConnectorStatus, PrivateAccessStatus } from "@/lib/types";
import { FieldLabel } from "./PrivateAccessConfig";

interface Props {
  enabled: boolean;
  cidrs: string;
  runtime?: PrivateAccessStatus;
  connectorStatus?: PrivateAccessConnectorStatus;
  onChange: (patch: { homeNetworkEnabled?: boolean; homeNetworkCidrs?: string }) => void;
}

export function PrivateAccessHomeNetwork({ enabled, cidrs, runtime, connectorStatus, onChange }: Props) {
  const { t } = useTranslation();
  const mode = connectorStatus?.mode;
  const label = mode === "direct"
    ? t("proxy.form.privateHomeNetworkDirect")
    : mode === "wireguard"
      ? t("proxy.form.privateHomeNetworkWireGuard")
      : mode === "inactive"
        ? t("proxy.form.privateHomeNetworkInactive")
        : mode === "unknown"
          ? t("proxy.form.privateHomeNetworkUnknown")
          : t("proxy.form.privateHomeNetworkPending");
  const color = mode === "direct" ? "green" : mode === "wireguard" ? "blue" : "orange";
  const interfaceDetail = runtime?.interface
    ? `${runtime.interface} · ${runtime.interface_addresses.join(", ") || "-"}`
    : runtime?.probe_error || "-";

  return <div className="space-y-3 rounded-md border border-emerald-500/25 bg-emerald-500/5 p-3">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <Checkbox checked={enabled} onChange={(event) => onChange({ homeNetworkEnabled: event.target.checked })}>
        {t("proxy.form.privateHomeNetworkEnabled")}
      </Checkbox>
      {enabled ? <div className="flex items-center gap-2 text-xs text-gray-500 dark:text-gray-400"><span>{connectorStatus ? t("proxy.form.privateHomeNetworkApplied") : t("proxy.form.privateHomeNetworkPending")}</span><Tag color={color}>{label}</Tag></div> : null}
    </div>
    {enabled ? <label className="block space-y-1">
      <FieldLabel>{t("proxy.form.privateHomeNetworkCidrs")}</FieldLabel>
      <Input size="small" value={cidrs} placeholder="10.8.28.0/24" onChange={(event) => onChange({ homeNetworkCidrs: event.target.value })} />
      <span className="block text-xs text-gray-500 dark:text-gray-400">{t("proxy.form.privateHomeNetworkHint")}</span>
    </label> : null}
    {enabled && connectorStatus ? <p className="text-xs text-gray-500 dark:text-gray-400">{interfaceDetail}{connectorStatus.matched_cidr ? ` · ${connectorStatus.matched_cidr}` : ""}</p> : null}
  </div>;
}
