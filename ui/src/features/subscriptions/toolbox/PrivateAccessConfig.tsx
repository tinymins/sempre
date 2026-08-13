import { Tooltip, WarningOutlined } from "@acme/components";
import { parse as parseJsonc } from "jsonc-parser";
import { message } from "@/lib/message";

export type ConnectorType =
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

export interface PrivateConnectorForm {
  enabled: boolean;
  tag: string;
  type: ConnectorType;
  address: string;
  privateKey: string;
  peerAddress: string;
  peerPort: number | null;
  transportEndpointRef: string;
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

export const CONNECTOR_TYPES: ConnectorType[] = [
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

export const COMMON_OUTBOUND_KEYS = new Set([
  "type",
  "tag",
  "server",
  "server_port",
  "serverPort",
  "uuid",
  "username",
  "password",
]);

export const splitList = (value: string): string[] =>
  value
    .split(/[\n,]/)
    .map((item) => item.trim())
    .filter(Boolean);

export const withDefaultCidrPrefix = (value: string): string =>
  value.includes("/") ? value : `${value}/${value.includes(":") ? 128 : 32}`;

export const splitCidrList = (value: string): string[] =>
  splitList(value).map(withDefaultCidrPrefix);

export const joinList = (value: unknown): string => {
  if (Array.isArray(value)) {
    return value
      .filter((item): item is string => typeof item === "string")
      .join(", ");
  }
  if (typeof value === "string") return value;
  return "";
};

export const emptyConnector = (): PrivateConnectorForm => ({
  enabled: true,
  tag: "",
  type: "wireguard",
  address: "",
  privateKey: "",
  peerAddress: "",
  peerPort: null,
  transportEndpointRef: "",
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

export const parseConfig = (value?: string) => {
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
          transportEndpointRef:
            typeof connector.transport_endpoint_ref === "string"
              ? connector.transport_endpoint_ref
              : typeof connector.tunnel_forward_id === "string"
                ? connector.tunnel_forward_id
                : typeof connector.tunnelForwardId === "string"
                  ? connector.tunnelForwardId
                  : "",
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

export const serializeConfig = (enabled: boolean, connectors: PrivateConnectorForm[]) => {
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
      if (connector.transportEndpointRef.trim()) {
        base.transport_endpoint_ref = connector.transportEndpointRef.trim();
      }
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

export const FieldLabel = ({ children }: { children: string }) => (
  <span className="text-xs text-gray-500 dark:text-gray-400">{children}</span>
);

export const connectorTypeLabel = (
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
