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
import { parse as parseJsonc } from "jsonc-parser";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { message } from "@/lib/message";

interface Props {
  value?: string;
  onChange?: (value: string) => void;
}

type ConnectorType =
  | "wireguard"
  | "vmess"
  | "vless"
  | "trojan"
  | "socks"
  | "http"
  | "ssh"
  | "hysteria2"
  | "tuic"
  | "anytls";

interface PrivateConnectorForm {
  enabled: boolean;
  tag: string;
  type: ConnectorType;
  address: string;
  privateKey: string;
  peerAddress: string;
  peerPort: number | null;
  publicKey: string;
  preSharedKey: string;
  allowedIps: string;
  persistentKeepaliveInterval: number | null;
  server: string;
  serverPort: number | null;
  uuid: string;
  username: string;
  password: string;
  extraOutboundJson: string;
  routeCidrs: string;
  routeDomainSuffixes: string;
  dnsDomainSuffixes: string;
  dnsServer: string;
  dnsServerPort: number | null;
}

const CONNECTOR_TYPES: ConnectorType[] = [
  "wireguard",
  "vmess",
  "vless",
  "trojan",
  "socks",
  "http",
  "ssh",
  "hysteria2",
  "tuic",
  "anytls",
];

const COMMON_OUTBOUND_KEYS = new Set([
  "type",
  "tag",
  "server",
  "server_port",
  "serverPort",
  "uuid",
  "username",
  "password",
]);

const splitList = (value: string): string[] =>
  value
    .split(/[\n,]/)
    .map((item) => item.trim())
    .filter(Boolean);

const withDefaultCidrPrefix = (value: string): string =>
  value.includes("/") ? value : `${value}/${value.includes(":") ? 128 : 32}`;

const splitCidrList = (value: string): string[] =>
  splitList(value).map(withDefaultCidrPrefix);

const joinList = (value: unknown): string => {
  if (Array.isArray(value)) {
    return value
      .filter((item): item is string => typeof item === "string")
      .join(", ");
  }
  if (typeof value === "string") return value;
  return "";
};

const emptyConnector = (): PrivateConnectorForm => ({
  enabled: true,
  tag: "",
  type: "wireguard",
  address: "",
  privateKey: "",
  peerAddress: "",
  peerPort: null,
  publicKey: "",
  preSharedKey: "",
  allowedIps: "",
  persistentKeepaliveInterval: 25,
  server: "",
  serverPort: null,
  uuid: "",
  username: "",
  password: "",
  extraOutboundJson: "",
  routeCidrs: "",
  routeDomainSuffixes: "",
  dnsDomainSuffixes: "",
  dnsServer: "",
  dnsServerPort: 53,
});

const parseConfig = (value?: string) => {
  if (!value?.trim()) {
    return { enabled: false, connectors: [] };
  }

  try {
    const parsed = parseJsonc(value) as {
      enabled?: boolean;
      connectors?: Array<Record<string, unknown>>;
    };
    const connectors =
      parsed.connectors?.map((connector, index): PrivateConnectorForm => {
        const endpoint = connector.endpoint as
          | Record<string, unknown>
          | undefined;
        const outbound = connector.outbound as
          | Record<string, unknown>
          | undefined;
        const routes = connector.routes as Record<string, unknown> | undefined;
        const dnsRules = Array.isArray(connector.dns)
          ? (connector.dns as Array<Record<string, unknown>>)
          : [];
        const dns = dnsRules[0] ?? {};
        const peers = Array.isArray(endpoint?.peers)
          ? (endpoint?.peers as Array<Record<string, unknown>>)
          : [];
        const peer = peers[0] ?? {};
        const rawType =
          typeof connector.type === "string" ? connector.type : "wireguard";
        const outboundType =
          typeof outbound?.type === "string" ? outbound.type : rawType;
        const type = CONNECTOR_TYPES.includes(outboundType as ConnectorType)
          ? (outboundType as ConnectorType)
          : "vmess";
        const extraOutbound: Record<string, unknown> = {};
        if (outbound) {
          for (const [key, extraValue] of Object.entries(outbound)) {
            if (!COMMON_OUTBOUND_KEYS.has(key)) {
              extraOutbound[key] = extraValue;
            }
          }
        }

        return {
          ...emptyConnector(),
          enabled: connector.enabled !== false,
          tag:
            typeof connector.tag === "string"
              ? connector.tag
              : `private-access-${index + 1}`,
          type: rawType === "wireguard" ? "wireguard" : type,
          address: joinList(endpoint?.address),
          privateKey:
            typeof endpoint?.private_key === "string"
              ? endpoint.private_key
              : typeof endpoint?.privateKey === "string"
                ? endpoint.privateKey
                : "",
          peerAddress:
            typeof peer.address === "string" ? peer.address : "",
          peerPort: typeof peer.port === "number" ? peer.port : null,
          publicKey:
            typeof peer.public_key === "string"
              ? peer.public_key
              : typeof peer.publicKey === "string"
                ? peer.publicKey
                : "",
          preSharedKey:
            typeof peer.pre_shared_key === "string"
              ? peer.pre_shared_key
              : typeof peer.preSharedKey === "string"
                ? peer.preSharedKey
                : "",
          allowedIps: joinList(peer.allowed_ips ?? peer.allowedIps),
          persistentKeepaliveInterval:
            typeof peer.persistent_keepalive_interval === "number"
              ? peer.persistent_keepalive_interval
              : typeof peer.persistentKeepaliveInterval === "number"
                ? peer.persistentKeepaliveInterval
                : null,
          server: typeof outbound?.server === "string" ? outbound.server : "",
          serverPort:
            typeof outbound?.server_port === "number"
              ? outbound.server_port
              : typeof outbound?.serverPort === "number"
                ? outbound.serverPort
                : null,
          uuid: typeof outbound?.uuid === "string" ? outbound.uuid : "",
          username:
            typeof outbound?.username === "string" ? outbound.username : "",
          password:
            typeof outbound?.password === "string" ? outbound.password : "",
          extraOutboundJson:
            Object.keys(extraOutbound).length > 0
              ? JSON.stringify(extraOutbound, null, 2)
              : "",
          routeCidrs: joinList(routes?.ipCidrs ?? routes?.ip_cidr),
          routeDomainSuffixes: joinList(
            routes?.domainSuffixes ?? routes?.domain_suffix,
          ),
          dnsDomainSuffixes: joinList(
            dns.domainSuffixes ?? dns.domain_suffix,
          ),
          dnsServer: typeof dns.server === "string" ? dns.server : "",
          dnsServerPort:
            typeof dns.serverPort === "number"
              ? dns.serverPort
              : typeof dns.server_port === "number"
                ? dns.server_port
                : 53,
        };
      }) ?? [];

    return {
      enabled: parsed.enabled === true,
      connectors,
    };
  } catch {
    return { enabled: false, connectors: [] };
  }
};

const serializeConfig = (enabled: boolean, connectors: PrivateConnectorForm[]) => {
  const outputConnectors = connectors.map((connector) => {
    const routes: Record<string, string[]> = {};
    const routeCidrs = splitCidrList(connector.routeCidrs);
    const routeDomainSuffixes = splitList(connector.routeDomainSuffixes);
    if (routeCidrs.length > 0) routes.ipCidrs = routeCidrs;
    if (routeDomainSuffixes.length > 0) {
      routes.domainSuffixes = routeDomainSuffixes;
    }

    const base: Record<string, unknown> = {
      enabled: connector.enabled,
      tag: connector.tag.trim(),
      type: connector.type,
    };

    if (Object.keys(routes).length > 0) {
      base.routes = routes;
    }

    const dnsDomainSuffixes = splitList(connector.dnsDomainSuffixes);
    const dnsServer = connector.dnsServer.trim();
    const dnsServerPort = connector.dnsServerPort ?? 53;
    // Every DNS field is user-owned state: keep partial input instead of
    // silently dropping it while the connector is being configured.
    const hasDnsConfig =
      dnsServer.length > 0 ||
      dnsDomainSuffixes.length > 0 ||
      dnsServerPort !== 53;
    if (hasDnsConfig) {
      base.dns = [
        {
          tag: `${connector.tag.trim() || "private-access"}-dns`,
          domainSuffixes: dnsDomainSuffixes,
          server: dnsServer,
          serverPort: dnsServerPort,
        },
      ];
    }

    if (connector.type === "wireguard") {
      base.endpoint = {
        address: splitCidrList(connector.address),
        privateKey: connector.privateKey.trim(),
        peers: [
          {
            address: connector.peerAddress.trim(),
            port: connector.peerPort ?? 0,
            publicKey: connector.publicKey.trim(),
            preSharedKey: connector.preSharedKey.trim(),
            allowedIps: splitCidrList(connector.allowedIps),
            persistentKeepaliveInterval:
              connector.persistentKeepaliveInterval ?? 25,
          },
        ],
      };
      return base;
    }

    let extra: Record<string, unknown> = {};
    if (connector.extraOutboundJson.trim()) {
      try {
        extra = parseJsonc(connector.extraOutboundJson) as Record<
          string,
          unknown
        >;
      } catch {
        message.error("extra outbound JSON 格式错误");
      }
    }
    base.outbound = {
      ...extra,
      type: connector.type,
      server: connector.server.trim(),
      serverPort: connector.serverPort ?? 0,
      ...(connector.uuid.trim() ? { uuid: connector.uuid.trim() } : {}),
      ...(connector.username.trim()
        ? { username: connector.username.trim() }
        : {}),
      ...(connector.password.trim()
        ? { password: connector.password.trim() }
        : {}),
    };
    return base;
  });

  return JSON.stringify(
    {
      enabled,
      connectors: outputConnectors,
    },
    null,
    2,
  );
};

const FieldLabel = ({ children }: { children: string }) => (
  <span className="text-xs text-gray-500 dark:text-gray-400">{children}</span>
);

const connectorTypeLabel = (
  type: ConnectorType,
  wireguardWarning: string,
) => {
  if (type !== "wireguard") return type;
  return (
    <span className="inline-flex min-w-0 items-center gap-1">
      <span>WireGuard</span>
      <Tooltip title={wireguardWarning}>
        <span className="inline-flex text-amber-500">
          <WarningOutlined />
        </span>
      </Tooltip>
    </span>
  );
};

const PrivateAccessEditor = ({ value, onChange }: Props) => {
  const { t } = useTranslation();
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
                    onChange={(peerPort) =>
                      updateConnector(index, { peerPort })
                    }
                    className="w-full"
                  />
                </label>
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

export default PrivateAccessEditor;
