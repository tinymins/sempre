import { Checkbox, Input, InputNumber, Select } from '@acme/components'
import { useRef, useState } from 'react'
import { useI18n } from '../i18n/provider'

import { initial, manualContent, type Node } from './manual-proxy'

export function ManualProxyFields({ onChange, onCommit, disabled }: { onChange: (content: string) => void; onCommit?: (content: string) => void; disabled: boolean }) {
  const { t } = useI18n()
  const [node, setNode] = useState(initial)
  const latest = useRef(initial)
  const patch = (patch: Partial<Node>, immediate = false) => { const next = { ...node, ...patch }; latest.current = next; setNode(next); onChange(manualContent(next)); if (immediate) onCommit?.(manualContent(next)) }
  const field = (key: keyof Node, label: string, secret = false) => <label className="block space-y-1 text-sm">{label}<Input disabled={disabled} type={secret ? 'password' : 'text'} value={String(node[key])} onChange={event => patch({ [key]: event.target.value })} /></label>
  return <div className="space-y-3" onBlur={event => { if (event.target instanceof HTMLInputElement && event.target.type !== 'checkbox') onCommit?.(manualContent(latest.current)) }}>
    <label className="block space-y-1 text-sm">{t('common.protocol')}<Select disabled={disabled} value={node.type} options={['http', 'socks5', 'ss', 'vmess', 'vless', 'trojan', 'hysteria2', 'tuic'].map(value => ({ value, label: value }))} onChange={value => patch({ type: String(value), tls: !['socks5', 'ss', 'http'].includes(String(value)) }, true)} className="w-full" /></label>
    <div className="grid gap-3 sm:grid-cols-2">{field('server', t('proxies.server'))}<label className="block space-y-1 text-sm">{t('proxies.port')}<InputNumber disabled={disabled} min={1} max={65535} value={node.port} onChange={value => patch({ port: value ?? 443 })} className="w-full" /></label></div>
    {['http', 'socks5'].includes(node.type) ? field('username', t('proxies.username')) : null}
    {['vmess', 'vless', 'tuic'].includes(node.type) ? field('uuid', 'UUID', true) : null}
    {['ss', 'trojan', 'hysteria2', 'tuic', 'http', 'socks5'].includes(node.type) ? field('password', t('common.password'), true) : null}
    {node.type === 'ss' ? field('cipher', t('proxies.cipher')) : null}
    {!['ss', 'socks5', 'hysteria2', 'tuic'].includes(node.type) ? <Checkbox disabled={disabled} checked={node.tls} onChange={event => patch({ tls: event.target.checked, reality: false }, true)}>TLS</Checkbox> : null}
    {node.tls ? field('servername', 'SNI') : null}
    {['vmess', 'vless', 'trojan'].includes(node.type) ? <label className="block space-y-1 text-sm">{t('proxies.transport')}<Select disabled={disabled} value={node.network} options={['tcp', 'ws', 'grpc'].map(value => ({ value, label: value }))} onChange={value => patch({ network: String(value) }, true)} className="w-full" /></label> : null}
    {node.network === 'ws' && ['vmess', 'vless', 'trojan'].includes(node.type) ? <div className="grid gap-3 sm:grid-cols-2">{field('path', t('proxies.wsPath'))}{field('host', t('proxies.wsHost'))}</div> : null}
    {node.network === 'grpc' && ['vmess', 'vless', 'trojan'].includes(node.type) ? field('service', t('proxies.grpcService')) : null}
    {node.type === 'vless' && node.tls ? <><Checkbox disabled={disabled} checked={node.reality} onChange={event => patch({ reality: event.target.checked }, true)}>REALITY</Checkbox>{field('flow', 'Flow')}{node.reality ? <>{field('publicKey', t('proxies.realityPublicKey'))}{field('shortId', t('proxies.realityShortId'))}</> : null}</> : null}
  </div>
}
