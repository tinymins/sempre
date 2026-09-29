import {
  Button,
  CodeEditor,
  Collapse,
  Checkbox,
  DeleteOutlined,
  ImportOutlined,
  Input,
  InputNumber,
  PlusOutlined,
  Select,
  TextArea,
  Tooltip,
  WarningOutlined,
} from "@acme/components";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useEditorI18n as useTranslation } from "./i18n";
import { editJsonc, readJsoncObject } from "./jsonc";
import { appendConnector, patchConnector } from "./private-edit";
import { FieldLabel, connectorTypeLabel } from "./private-labels";
import { WireGuardImportModal } from "./WireGuardImportModal";
import {
  CONNECTOR_TYPES,
  type ConnectorType,
  type PrivateConnectorForm,
  parseConfig,
  splitList,
} from "./private-model";

export interface PrivateAccessEditorProps {
  value?: string;
  readOnly?: boolean;
  onChange?: (value: string) => void;
  renderTransport?: (connector: PrivateConnectorForm, onChange: (patch: Partial<PrivateConnectorForm>) => void) => ReactNode;
  renderHomeNetwork?: (connector: PrivateConnectorForm, index: number, onChange: (patch: Partial<PrivateConnectorForm>) => void) => ReactNode;
  variant?: "default" | "simple";
}

export const PrivateAccessEditor = ({ value, readOnly, onChange, renderTransport, renderHomeNetwork, variant = "default" }: PrivateAccessEditorProps) => {
  const { t } = useTranslation();
  const [state, setState] = useState(() => parseConfig(value));
  const [importIndex, setImportIndex] = useState<number | null>(null);
  const document = readJsoncObject(value);
  const invalid = document.error || (document.object?.connectors !== undefined && (!Array.isArray(document.object.connectors) || document.object.connectors.some(item => !item || typeof item !== 'object' || Array.isArray(item))));
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

  const emit = (nextValue: string, connectors = state.connectors) => {
    if (invalid || readOnly) return;
    setState({ enabled: readJsoncObject(nextValue).object?.enabled === true, connectors });
    lastEmittedValueRef.current = nextValue;
    onChange?.(nextValue);
  };
  const updateConnector = (index: number, patch: Partial<PrivateConnectorForm>) => {
    if (invalid || readOnly) return;
    emit(patchConnector(value, index, patch), state.connectors.map((connector, position) => position === index ? { ...connector, ...patch } : connector));
  };
  const removeConnector = (index: number) => {
    emit(editJsonc(value, ['connectors', index], undefined), state.connectors.filter((_, position) => position !== index));
  };
  const addConnector = () => {
    const next = appendConnector(value);
    emit(next, parseConfig(next).connectors);
  };

  return (
    <div className="space-y-3">
      {invalid ? <p role="alert" className="text-sm text-red-600">{t("private.invalid")}</p> : null}
      <fieldset disabled={invalid || readOnly} className="m-0 min-w-0 space-y-3 border-0 p-0">
      <div className={variant === "simple" ? "flex items-center gap-2 px-1 py-1" : "flex items-center gap-2 rounded-lg border border-gray-200 bg-white p-3 dark:border-gray-700 dark:bg-[#151515]"}>
        <Checkbox
          checked={state.enabled}
          onChange={(event) => emit(editJsonc(value, ["enabled"], event.target.checked))}
        >
          {t("proxy.form.privateAccessEnabled")}
        </Checkbox>
      </div>

      {state.connectors.map((connector, index) => (
        <div
          key={`private-connector-${index}`}
          className={variant === "simple"
            ? `rounded-xl p-4 transition-colors ${connector.enabled ? "bg-black/[0.025] dark:bg-white/[0.035]" : "bg-black/[0.02] opacity-60 dark:bg-white/[0.02]"}`
            : `rounded-lg border p-3 transition-colors ${connector.enabled
              ? "border-gray-200 bg-white dark:border-gray-600 dark:bg-[#1a1a1a]"
              : "border-dashed border-gray-300 bg-gray-50 opacity-60 dark:border-gray-700 dark:bg-[#111]"}`}
        >
          <div className="space-y-3">
            <div className="flex flex-wrap items-center gap-2">
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
                className="min-w-48 flex-1"
              />
              <Select
                size="small"
                value={connector.type}
                options={connectorTypeOptions}
                onChange={(nextType) =>
                  updateConnector(index, { type: nextType as ConnectorType })
                }
                className="w-full shrink-0 sm:w-[150px]"
              />
              <Tooltip title={t("proxy.form.privateWgImportTooltip")}>
                <span className="inline-flex shrink-0">
                  <Button
                    variant="text"
                    size="small"
                    disabled={connector.type !== "wireguard"}
                    aria-label={t("proxy.form.privateWgImportTooltip")}
                    icon={<ImportOutlined />}
                    onClick={() => setImportIndex(index)}
                  />
                </span>
              </Tooltip>
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
                {renderTransport?.(connector, patch => updateConnector(index, patch))}
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

              </div>
            )}

            {connector.type === "wireguard" ? renderHomeNetwork?.(connector, index, patch => updateConnector(index, patch)) : null}

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
      </fieldset>
      <Collapse items={[{ key: "document", label: t("filter.advanced"), children: <CodeEditor readOnly={readOnly} ariaLabel={t("editor.tabPrivate")} value={value ?? ""} height={320} onChange={onChange} /> }]} />
      <WireGuardImportModal
        open={importIndex !== null}
        onCancel={() => setImportIndex(null)}
        onImport={(patch) => {
          if (importIndex === null) return;
          updateConnector(importIndex, patch);
          setImportIndex(null);
        }}
      />
    </div>
  );
};
