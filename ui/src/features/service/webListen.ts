export interface WebSettings {
  listen: string
  listen_addresses: string[]
  local_url: string
  password_set: boolean
  password_warning: boolean
  interfaces: Array<{ interface: string; address: string }>
}

export function splitListen(value: string) {
  const url = new URL(`http://${value}`)
  return { host: url.hostname.replace(/^\[|\]$/g, ''), port: url.port || '80' }
}

export function normalizeListenHost(value: string) {
  const host = value.trim().replace(/^\[|\]$/g, '')
  if (!host.includes(':') && !/^\d+\.\d+\.\d+\.\d+$/.test(host)) throw new Error('Enter an IPv4 or IPv6 address')
  const url = new URL(`http://${host.includes(':') ? `[${host}]` : host}:33211`)
  return url.hostname.replace(/^\[|\]$/g, '')
}

export function selectListenHosts(previous: string[], values: string[]) {
  const hosts = [...new Set(values.map(normalizeListenHost))]
  const added = hosts.find((host) => !previous.includes(host))
  if (!added) return hosts
  const wildcard = added.includes(':') ? '::' : '0.0.0.0'
  return hosts.filter((host) => host === added || (added === wildcard ? host.includes(':') !== added.includes(':') : host !== wildcard))
}

export function joinListen(host: string, port: string) {
  if (!/^\d+$/.test(port) || Number(port) < 1 || Number(port) > 65535) throw new Error('Port must be between 1 and 65535')
  return `${host.includes(':') ? `[${host}]` : host}:${Number(port)}`
}

export function loopback(host: string) { return host === '::1' || host === 'localhost' || host.startsWith('127.') }

export function listenNavigation(baseURL: string, addresses: string[]) {
  const url = new URL(baseURL)
  const current = url.hostname.replace(/^\[|\]$/g, '')
  const selected = addresses.map(splitListen)
  const keepsCurrent = selected.some(({ host }) => host === current || (host === '::' && current.includes(':')) || (host === '0.0.0.0' && !current.includes(':')))
  const target = keepsCurrent ? current : selected.find(({ host }) => !loopback(host) && host !== '0.0.0.0' && host !== '::')?.host
    ?? (loopback(current) ? selected[0]?.host : undefined)
  if (!target) return null
  const destination = target === '0.0.0.0' ? '127.0.0.1' : target === '::' ? '::1' : target
  url.hostname = destination.includes(':') ? `[${destination}]` : destination
  url.port = selected[0].port
  return url.toString().replace(/\/$/, '')
}
