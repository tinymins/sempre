import { parse as parseJsonc } from "jsonc-parser";
export type ConnectorType =
  | "tailscale"
  | "outbound"
  | "v2ray"
  | "xray"
  | "socks5"
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
  homeNetworkEnabled: boolean;
  homeNetworkIds: string[];
  server: string;
  serverPort: number | null;
  uuid: string;
  username: string;
  password: string;
  routeCidrs: string;
  routeDomainSuffixes: string;
  dnsDomainSuffixes: string;
  dnsServer: string;
  dnsServerPort: number | null;
}

export const CONNECTOR_TYPES: ConnectorType[] = [
  "tailscale",
  "outbound",
  "v2ray",
  "xray",
  "socks5",
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
  homeNetworkEnabled: false,
  homeNetworkIds: [],
  server: "",
  serverPort: null,
  uuid: "",
  username: "",
  password: "",
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
        const homeNetwork = connector.homeNetwork as
          | Record<string, unknown>
          | undefined;
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
          : rawType as ConnectorType;

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
          homeNetworkEnabled: homeNetwork?.enabled === true,
          homeNetworkIds: Array.isArray(homeNetwork?.networkIds)
            ? homeNetwork.networkIds.filter((item): item is string => typeof item === "string")
            : [],
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
