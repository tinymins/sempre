import {
  Button,
  Checkbox,
  DeleteOutlined,
  Input,
  InputNumber,
  PlusOutlined,
  Select,
  TextArea,
  Tooltip,
  WarningOutlined,
} from "@acme/components";
import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { useOptionalSession } from "@/lib/session";
import type { TunnelStatus } from "@/lib/types";
import {
  CONNECTOR_TYPES,
  type ConnectorType,
  connectorTypeLabel,
  emptyConnector,
  FieldLabel,
  type PrivateConnectorForm,
  parseConfig,
  serializeConfig,
  splitList,
} from "./PrivateAccessConfig";

interface Props {
  value?: string;
  onChange?: (value: string) => void;
}

const PrivateAccessEditor = ({ value, onChange }: Props) => {
  const { t } = useTranslation();
  const sessionContext = useOptionalSession();
  const session = sessionContext?.session;
  const [state, setState] = useState(() => parseConfig(value));
  const lastEmittedValueRef = useRef<string | undefined>(undefined);
  const connectorTypeOptions = useMemo(
    () =>
      CONNECTOR_TYPES.map((type) => ({
        value: type,
        label: connectorTypeLabel(type, t("proxy.form.privateWgReuseWarning")),
      })),
    [t],
  );
  useEffect(() => {
    if (value === lastEmittedValueRef.current) return;
    setState(parseConfig(value));
  }, [value]);

  const emit = (enabled: boolean, connectors: PrivateConnectorForm[]) => {
    setState({ enabled, connectors });
    const nextValue = serializeConfig(enabled, connectors);
    lastEmittedValueRef.current = nextValue;
    onChange?.(nextValue);
  };

  const updateConnector = (
    index: number,
    patch: Partial<PrivateConnectorForm>,
  ) => {
    emit(
      state.enabled,
      state.connectors.map((connector, itemIndex) =>
        itemIndex === index ? { ...connector, ...patch } : connector,
      ),
    );
  };

  const removeConnector = (index: number) => {
    const next = state.connectors.filter((_, itemIndex) => itemIndex !== index);
    emit(state.enabled, next);
  };

  const addConnector = () => {
    emit(state.enabled, [
      ...state.connectors,
      emptyConnector(),
    ]);
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2 rounded-lg border border-gray-200 bg-white p-3 dark:border-gray-700 dark:bg-[#151515]">
        <Checkbox
          checked={state.enabled}
          onChange={(event) => emit(event.target.checked, state.connectors)}
        >
          {t("proxy.form.privateAccessEnabled")}
        </Checkbox>
      </div>

      {state.connectors.map((connector, index) => (
        <div
          key={`private-connector-${index}`}
          className={`rounded-lg border p-3 transition-colors ${
            connector.enabled
              ? "border-gray-200 bg-white dark:border-gray-600 dark:bg-[#1a1a1a]"
              : "border-dashed border-gray-300 bg-gray-50 opacity-60 dark:border-gray-700 dark:bg-[#111]"
          }`}
        >
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <Tooltip
                title={
                  connector.enabled
                    ? t("proxy.form.privateConnectorEnabled")
                    : t("proxy.form.privateConnectorDisabled")
                }
              >
                <Checkbox
                  checked={connector.enabled}
                  onChange={(event) =>
                    updateConnector(index, { enabled: event.target.checked })
                  }
                />
              </Tooltip>
              <Input
                size="small"
                value={connector.tag}
                placeholder={`private-access-${index + 1}`}
                onChange={(event) =>
                  updateConnector(index, { tag: event.target.value })
                }
                className="flex-1 min-w-0"
              />
              <Select
                size="small"
                value={connector.type}
                options={connectorTypeOptions}
                onChange={(nextType) =>
                  updateConnector(index, { type: nextType as ConnectorType })
                }
                className="w-[150px] shrink-0"
              />
              <Button
                variant="text"
                size="small"
                danger
                icon={<DeleteOutlined />}
                onClick={() => removeConnector(index)}
                className="shrink-0"
              />
            </div>

            {connector.type === "wireguard" ? (
              <div className="grid grid-cols-1 gap-2 md:grid-cols-3">
                <label className="space-y-1">
                  <FieldLabel>{t("proxy.form.privateWgAddress")}</FieldLabel>
                  <Input
                    size="small"
                    value={connector.address}
                    placeholder="192.0.2.2/32, 2001:db8::2/128"
                    onChange={(event) =>
                      updateConnector(index, { address: event.target.value })
                    }
                  />
                </label>
                <label className="space-y-1 md:col-span-2">
                  <FieldLabel>{t("proxy.form.privateWgPrivateKey")}</FieldLabel>
                  <Input
                    size="small"
                    value={connector.privateKey}
                    onChange={(event) =>
                      updateConnector(index, { privateKey: event.target.value })
                    }
                  />
                </label>
                <label className="space-y-1">
                  <FieldLabel>{t("proxy.form.privateWgPeerAddress")}</FieldLabel>
                  <Input
                    size="small"
                    value={connector.peerAddress}
                    disabled={Boolean(connector.transportEndpointRef)}
                    placeholder="vpn.example.com"
                    onChange={(event) =>
                      updateConnector(index, {
                        peerAddress: event.target.value,
                      })
                    }
                  />
                </label>
                <label className="space-y-1">
                  <FieldLabel>{t("proxy.form.privateWgPeerPort")}</FieldLabel>
                  <InputNumber
                    size="small"
                    min={1}
                    max={65535}
                    value={connector.peerPort}
                    disabled={Boolean(connector.transportEndpointRef)}
                    onChange={(peerPort) =>
                      updateConnector(index, { peerPort })
                    }
                    className="w-full"
                  />
                </label>
                {session ? <TransportTunnelSelect session={session} value={connector.transportEndpointRef} onChange={(transportEndpointRef) => updateConnector(index, { transportEndpointRef })} /> : null}
                <label className="space-y-1">
                  <FieldLabel>{t("proxy.form.privateWgKeepalive")}</FieldLabel>
                  <InputNumber
                    size="small"
                    min={0}
                    max={3600}
                    value={connector.persistentKeepaliveInterval}
                    onChange={(persistentKeepaliveInterval) =>
                      updateConnector(index, {
                        persistentKeepaliveInterval,
                      })
                    }
                    className="w-full"
                  />
                </label>
                <label className="space-y-1 md:col-span-2">
                  <FieldLabel>{t("proxy.form.privateWgPublicKey")}</FieldLabel>
                  <Input
                    size="small"
                    value={connector.publicKey}
                    onChange={(event) =>
                      updateConnector(index, { publicKey: event.target.value })
                    }
                  />
                </label>
                <label className="space-y-1">
                  <FieldLabel>{t("proxy.form.privateWgPresharedKey")}</FieldLabel>
                  <Input
                    size="small"
                    value={connector.preSharedKey}
                    onChange={(event) =>
                      updateConnector(index, {
                        preSharedKey: event.target.value,
                      })
                    }
                  />
                </label>
                <label className="space-y-1 md:col-span-3">
                  <FieldLabel>{t("proxy.form.privateWgAllowedIps")}</FieldLabel>
                  <TextArea
                    rows={2}
                    size="small"
                    value={connector.allowedIps}
                    placeholder={"192.0.2.0/24, 2001:db8::/32"}
                    onChange={(event) =>
                      updateConnector(index, { allowedIps: event.target.value })
                    }
                  />
                </label>
              </div>
            ) : (
              <div className="grid grid-cols-1 gap-2 md:grid-cols-4">
                <label className="space-y-1 md:col-span-2">
                  <FieldLabel>{t("proxy.form.privateOutboundServer")}</FieldLabel>
                  <Input
                    size="small"
                    value={connector.server}
                    onChange={(event) =>
                      updateConnector(index, { server: event.target.value })
                    }
                  />
                </label>
                <label className="space-y-1">
                  <FieldLabel>{t("proxy.form.privateOutboundPort")}</FieldLabel>
                  <InputNumber
                    size="small"
                    min={1}
                    max={65535}
                    value={connector.serverPort}
                    onChange={(serverPort) =>
                      updateConnector(index, { serverPort })
                    }
                    className="w-full"
                  />
                </label>
                <label className="space-y-1">
                  <FieldLabel>{t("proxy.form.privateOutboundUuid")}</FieldLabel>
                  <Input
                    size="small"
                    value={connector.uuid}
                    onChange={(event) =>
                      updateConnector(index, { uuid: event.target.value })
                    }
                  />
                </label>
                <label className="space-y-1">
                  <FieldLabel>{t("proxy.form.privateOutboundUsername")}</FieldLabel>
                  <Input
                    size="small"
                    value={connector.username}
                    onChange={(event) =>
                      updateConnector(index, { username: event.target.value })
                    }
                  />
                </label>
                <label className="space-y-1">
                  <FieldLabel>{t("proxy.form.privateOutboundPassword")}</FieldLabel>
                  <Input
                    size="small"
                    value={connector.password}
                    onChange={(event) =>
                      updateConnector(index, { password: event.target.value })
                    }
                  />
                </label>
                <label className="space-y-1 md:col-span-4">
                  <FieldLabel>{t("proxy.form.privateOutboundExtra")}</FieldLabel>
                  <TextArea
                    rows={3}
                    size="small"
                    value={connector.extraOutboundJson}
                    placeholder='{ "tls": { "enabled": true } }'
                    onChange={(event) =>
                      updateConnector(index, {
                        extraOutboundJson: event.target.value,
                      })
                    }
                  />
                </label>
              </div>
            )}

            <div className="grid grid-cols-1 gap-2 border-t border-gray-100 pt-3 dark:border-gray-800 md:grid-cols-2">
              <label className="space-y-1">
                <FieldLabel>{t("proxy.form.privateRouteCidrs")}</FieldLabel>
                <TextArea
                  rows={2}
                  size="small"
                  value={connector.routeCidrs}
                  placeholder={"198.51.100.0/24, 2001:db8:1::/48"}
                  onChange={(event) =>
                    updateConnector(index, { routeCidrs: event.target.value })
                  }
                />
              </label>
              <label className="space-y-1">
                <FieldLabel>{t("proxy.form.privateRouteDomains")}</FieldLabel>
                <TextArea
                  rows={2}
                  size="small"
                  value={connector.routeDomainSuffixes}
                  placeholder={"corp.example.com, internal.example.com\nhome.arpa"}
                  onChange={(event) =>
                    updateConnector(index, {
                      routeDomainSuffixes: event.target.value,
                    })
                  }
                />
              </label>
              <label className="space-y-1">
                <FieldLabel>{t("proxy.form.privateDnsDomains")}</FieldLabel>
                <TextArea
                  rows={2}
                  size="small"
                  value={connector.dnsDomainSuffixes}
                  placeholder={"service.example.com, home.arpa"}
                  onChange={(event) =>
                    updateConnector(index, {
                      dnsDomainSuffixes: event.target.value,
                    })
                  }
                />
              </label>
              <div className="grid grid-cols-[1fr_120px] gap-2">
                <label className="space-y-1">
                  <FieldLabel>{t("proxy.form.privateDnsServer")}</FieldLabel>
                  <Input
                    size="small"
                    value={connector.dnsServer}
                    placeholder="192.0.2.53"
                    onChange={(event) =>
                      updateConnector(index, { dnsServer: event.target.value })
                    }
                  />
                </label>
                <label className="space-y-1">
                  <FieldLabel>{t("proxy.form.privateDnsPort")}</FieldLabel>
                  <InputNumber
                    size="small"
                    min={1}
                    max={65535}
                    value={connector.dnsServerPort}
                    onChange={(dnsServerPort) =>
                      updateConnector(index, { dnsServerPort })
                    }
                    className="w-full"
                  />
                </label>
              </div>
              {connector.dnsServer.trim() &&
                splitList(connector.dnsDomainSuffixes).length === 0 && (
                  <div className="flex items-start gap-2 rounded-md bg-amber-50 px-3 py-2 text-xs text-amber-700 dark:bg-amber-900/20 dark:text-amber-300 md:col-span-2">
                    <span className="mt-0.5 inline-flex shrink-0">
                      <WarningOutlined />
                    </span>
                    <span>{t("proxy.form.privateDnsGlobalWarning")}</span>
                  </div>
                )}
            </div>
          </div>
        </div>
      ))}

      <Button
        variant="dashed"
        block
        icon={<PlusOutlined />}
        onClick={addConnector}
      >
        {t("proxy.form.addPrivateConnector")}
      </Button>
    </div>
  );
};

function TransportTunnelSelect({ session, value, onChange }: { session: NonNullable<ReturnType<typeof useOptionalSession>>['session']; value: string; onChange: (value: string) => void }) {
  const { t } = useTranslation();
  const tunnels = useQuery({ queryKey: ["tunnels"], queryFn: () => api<TunnelStatus>(session!, "/tunnels") });
  const options = (tunnels.data?.forwards ?? []).map((forward) => ({ value: forward.forward_id, label: `${forward.instance_name} / ${forward.forward_name} · ${forward.host}:${forward.port}` }));
  return <label className="space-y-1 md:col-span-3"><FieldLabel>{t("proxy.form.privateTunnelForward")}</FieldLabel><Select size="small" allowClear value={value || undefined} options={options} placeholder={t("proxy.form.privateTunnelDirect")} onChange={(forwardID) => onChange(forwardID || "")} className="w-full" /></label>;
}

export default PrivateAccessEditor;
