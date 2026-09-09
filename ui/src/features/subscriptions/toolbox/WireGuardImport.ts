import type { PrivateConnectorForm } from "./PrivateAccessConfig";

export type WireGuardImportErrorCode =
  | "empty"
  | "missingInterface"
  | "missingPeer"
  | "multiplePeers"
  | "missingRequiredFields"
  | "invalidEndpoint"
  | "invalidKeepalive";

export class WireGuardImportError extends Error {
  constructor(public readonly code: WireGuardImportErrorCode) {
    super(code);
  }
}

type Section = "interface" | "peer";

export function parseWireGuardImport(
  input: string,
): Partial<PrivateConnectorForm> {
  if (!input.trim()) throw new WireGuardImportError("empty");

  const values: Record<Section, Map<string, string[]>> = {
    interface: new Map(),
    peer: new Map(),
  };
  let section: Section | undefined;
  let peerCount = 0;

  for (const rawLine of input.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#") || line.startsWith(";")) continue;
    const sectionMatch = line.match(/^\[([^\]]+)]$/);
    if (sectionMatch) {
      const name = sectionMatch[1].trim().toLowerCase();
      section = name === "interface" ? "interface" : name === "peer" ? "peer" : undefined;
      if (section === "peer") peerCount += 1;
      continue;
    }
    if (!section) continue;
    const separator = line.indexOf("=");
    if (separator < 0) continue;
    const key = line.slice(0, separator).trim().toLowerCase();
    const value = line.slice(separator + 1).trim();
    const existing = values[section].get(key) ?? [];
    values[section].set(key, [...existing, value]);
  }

  if (values.interface.size === 0) {
    throw new WireGuardImportError("missingInterface");
  }
  if (peerCount === 0 || values.peer.size === 0) {
    throw new WireGuardImportError("missingPeer");
  }
  if (peerCount > 1) throw new WireGuardImportError("multiplePeers");

  const privateKey = first(values.interface, "privatekey");
  const addresses = list(values.interface, "address");
  const publicKey = first(values.peer, "publickey");
  const allowedIps = list(values.peer, "allowedips");
  const endpoint = first(values.peer, "endpoint");
  if (!privateKey || !addresses.length || !publicKey || !allowedIps.length || !endpoint) {
    throw new WireGuardImportError("missingRequiredFields");
  }

  const parsedEndpoint = parseEndpoint(endpoint);
  const keepaliveValue = first(values.peer, "persistentkeepalive");
  const keepalive = keepaliveValue === "" ? null : Number(keepaliveValue);
  if (
    keepaliveValue !== "" &&
    (!Number.isInteger(keepalive) || keepalive === null || keepalive < 0 || keepalive > 3600)
  ) {
    throw new WireGuardImportError("invalidKeepalive");
  }

  return {
    address: addresses.join(", "),
    privateKey,
    peerAddress: parsedEndpoint.address,
    peerPort: parsedEndpoint.port,
    transportEndpointRef: "",
    publicKey,
    preSharedKey: first(values.peer, "presharedkey"),
    allowedIps: allowedIps.join(", "),
    persistentKeepaliveInterval: keepalive ?? 25,
    dnsServer: list(values.interface, "dns")[0] ?? "",
    dnsServerPort: 53,
  };
}

function first(section: Map<string, string[]>, key: string): string {
  return section.get(key)?.[0]?.trim() ?? "";
}

function list(section: Map<string, string[]>, key: string): string[] {
  return (section.get(key) ?? [])
    .flatMap((value) => value.split(","))
    .map((value) => value.trim())
    .filter(Boolean);
}

function parseEndpoint(value: string): { address: string; port: number } {
  const bracketed = value.match(/^\[([^\]]+)]:(\d+)$/);
  const plain = value.match(/^(.+):(\d+)$/);
  const match = bracketed ?? plain;
  const address = match?.[1]?.trim() ?? "";
  const port = Number(match?.[2]);
  if (!address || !Number.isInteger(port) || port < 1 || port > 65535) {
    throw new WireGuardImportError("invalidEndpoint");
  }
  return { address, port };
}
