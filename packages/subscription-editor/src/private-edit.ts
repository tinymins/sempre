import { editJsonc, readJsoncObject } from './jsonc'
import { splitCidrList, splitList, type PrivateConnectorForm } from './private-model'

const fields: Record<keyof PrivateConnectorForm, (string | number)[]> = {
  enabled: ['enabled'], tag: ['tag'], type: ['type'],
  address: ['endpoint', 'address'], privateKey: ['endpoint', 'privateKey'],
  peerAddress: ['endpoint', 'peers', 0, 'address'], peerPort: ['endpoint', 'peers', 0, 'port'],
  transportEndpointRef: ['transport_endpoint_ref'],
  publicKey: ['endpoint', 'peers', 0, 'publicKey'], preSharedKey: ['endpoint', 'peers', 0, 'preSharedKey'],
  allowedIps: ['endpoint', 'peers', 0, 'allowedIps'], persistentKeepaliveInterval: ['endpoint', 'peers', 0, 'persistentKeepaliveInterval'],
  homeNetworkEnabled: ['homeNetwork', 'enabled'], homeNetworkIds: ['homeNetwork', 'networkIds'],
  server: ['outbound', 'server'], serverPort: ['outbound', 'serverPort'], uuid: ['outbound', 'uuid'],
  username: ['outbound', 'username'], password: ['outbound', 'password'],
  routeCidrs: ['routes', 'ipCidrs'], routeDomainSuffixes: ['routes', 'domainSuffixes'],
  dnsDomainSuffixes: ['dns', 0, 'domainSuffixes'], dnsServer: ['dns', 0, 'server'], dnsServerPort: ['dns', 0, 'serverPort'],
}
const aliases: Record<string, string> = {
  privateKey: 'private_key', publicKey: 'public_key', preSharedKey: 'pre_shared_key', allowedIps: 'allowed_ips',
  persistentKeepaliveInterval: 'persistent_keepalive_interval', ipCidrs: 'ip_cidr', domainSuffixes: 'domain_suffix', serverPort: 'server_port',
}

export function patchConnector(source: string | undefined, index: number, patch: Partial<PrivateConnectorForm>): string {
  let next = source ?? '{}'
  for (const [field, raw] of Object.entries(patch)) {
    const path = ['connectors', index, ...fields[field as keyof typeof fields]]
    const document = readJsoncObject(next)
    if (document.error) return next
    let parent: unknown = document.object
    for (const part of path.slice(0, -1)) parent = parent && typeof parent === 'object' ? (parent as Record<string | number, unknown>)[part] : undefined
    const key = path.at(-1) as string
    const alias = aliases[key]
    if (alias && parent && typeof parent === 'object' && alias in parent) path[path.length - 1] = alias
    const value = typeof raw === 'string'
      ? ['address', 'allowedIps', 'routeCidrs'].includes(field) ? splitCidrList(raw)
        : ['routeDomainSuffixes', 'dnsDomainSuffixes'].includes(field) ? splitList(raw) : raw.trim()
      : raw
    next = editJsonc(next, path, value ?? undefined)
    if (field === 'type' && !['wireguard', 'tailscale', 'outbound', 'v2ray', 'xray'].includes(String(raw))) {
      next = editJsonc(next, ['connectors', index, 'outbound', 'type'], raw)
    }
  }
  return next
}

export function appendConnector(source: string | undefined): string {
  return editJsonc(source, ['connectors', -1], { enabled: true, type: 'wireguard', tag: '', endpoint: { address: [], privateKey: '', peers: [{ persistentKeepaliveInterval: 25 }] } })
}
