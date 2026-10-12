export interface Node {
  type: string; server: string; port: number; username: string; password: string; uuid: string;
  cipher: string; tls: boolean; servername: string; network: string; path: string; host: string;
  service: string; reality: boolean; publicKey: string; shortId: string; flow: string
}
export const initial: Node = { type: 'vless', server: '', port: 443, username: '', password: '', uuid: '', cipher: 'aes-256-gcm', tls: true, servername: '', network: 'tcp', path: '/', host: '', service: '', reality: false, publicKey: '', shortId: '', flow: '' }

export function manualContent(node: Node): string {
  const proxy: Record<string, unknown> = { name: 'subscription-proxy', type: node.type, server: node.server, port: node.port }
  if (['vmess', 'vless', 'tuic'].includes(node.type)) proxy.uuid = node.uuid
  if (['ss', 'trojan', 'hysteria2', 'tuic', 'http', 'socks5'].includes(node.type) && node.password) proxy.password = node.password
  if (['http', 'socks5'].includes(node.type) && node.username) proxy.username = node.username
  if (node.type === 'ss') proxy.cipher = node.cipher
  if (node.type === 'vmess') { proxy.cipher = 'auto'; proxy.alterId = 0 }
  if (['hysteria2', 'tuic'].includes(node.type)) { if (node.servername) proxy.sni = node.servername }
  else if (node.tls) { proxy.tls = true; if (node.servername) proxy[['http', 'socks5'].includes(node.type) ? 'sni' : 'servername'] = node.servername }
  if (node.type === 'vless' && node.flow) proxy.flow = node.flow
  if (['vmess', 'vless', 'trojan'].includes(node.type) && node.network !== 'tcp') {
    proxy.network = node.network
    if (node.network === 'ws') proxy['ws-opts'] = { path: node.path, ...(node.host ? { headers: { Host: node.host } } : {}) }
    if (node.network === 'grpc') proxy['grpc-opts'] = { 'grpc-service-name': node.service }
  }
  if (node.reality && node.type === 'vless') { proxy['reality-opts'] = { 'public-key': node.publicKey, 'short-id': node.shortId }; proxy['client-fingerprint'] = 'chrome' }
  return JSON.stringify({ proxies: [proxy] }, null, 2)
}

export const defaultManualContent = () => manualContent(initial)
