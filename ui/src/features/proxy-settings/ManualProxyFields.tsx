import { useState } from 'react'
import { Checkbox, InputNumber, Select } from '@acme/components'
import { initialManualProxy, manualProxyContent, type ManualProxyNode } from '@acme/subscription-editor'
import { Field, Input } from '../../components/ui'

type Label = (zh: string, en: string) => string
const PROTOCOLS = ['http', 'socks5', 'ss', 'vmess', 'vless', 'trojan', 'hysteria2', 'tuic']

export function ManualProxyFields({ onChange, disabled, label }: { onChange: (content: string) => void; disabled: boolean; label: Label }) {
  const [node, setNode] = useState(initialManualProxy)
  const patch = (change: Partial<ManualProxyNode>) => {
    const next = { ...node, ...change }
    setNode(next)
    onChange(manualProxyContent(next))
  }
  const text = (key: keyof ManualProxyNode, name: string, secret = false) => <Field label={name}><Input disabled={disabled} type={secret ? 'password' : 'text'} autoComplete="off" value={String(node[key])} onChange={(event) => patch({ [key]: event.target.value })} /></Field>
  const streamed = ['vmess', 'vless', 'trojan'].includes(node.type)
  return <div className="space-y-3">
    <Field label={label('协议', 'Protocol')}><Select className="w-full" disabled={disabled} value={node.type} options={PROTOCOLS.map((value) => ({ value, label: value }))} onChange={(type) => patch({ type: String(type) })} /></Field>
    <div className="grid gap-3 sm:grid-cols-2">
      {text('server', label('服务器', 'Server'))}
      <Field label={label('端口', 'Port')}><InputNumber className="w-full" disabled={disabled} min={1} max={65535} value={node.port} onChange={(port) => patch({ port: Number(port) || 1 })} /></Field>
    </div>
    {['http', 'socks5'].includes(node.type) ? text('username', label('用户名', 'Username')) : null}
    {['vmess', 'vless', 'tuic'].includes(node.type) ? text('uuid', 'UUID', true) : null}
    {['ss', 'trojan', 'hysteria2', 'tuic', 'http', 'socks5'].includes(node.type) ? text('password', label('密码', 'Password'), true) : null}
    {node.type === 'ss' ? text('cipher', label('加密方式', 'Cipher')) : null}
    {!['ss', 'socks5', 'hysteria2', 'tuic'].includes(node.type) ? <Checkbox disabled={disabled} checked={node.tls} onChange={(event) => patch({ tls: event.target.checked, reality: false })}>TLS</Checkbox> : null}
    {node.tls ? text('servername', 'SNI') : null}
    {streamed ? <Field label={label('传输方式', 'Transport')}><Select className="w-full" disabled={disabled} value={node.network} options={['tcp', 'ws', 'grpc'].map((value) => ({ value, label: value }))} onChange={(network) => patch({ network: String(network) })} /></Field> : null}
    {streamed && node.network === 'ws' ? <div className="grid gap-3 sm:grid-cols-2">{text('path', label('WS 路径', 'WS path'))}{text('host', label('WS Host', 'WS host'))}</div> : null}
    {streamed && node.network === 'grpc' ? text('service', label('gRPC 服务名', 'gRPC service')) : null}
    {node.type === 'vless' && node.tls ? <>
      <Checkbox disabled={disabled} checked={node.reality} onChange={(event) => patch({ reality: event.target.checked })}>REALITY</Checkbox>
      {text('flow', 'Flow')}
      {node.reality ? <div className="grid gap-3 sm:grid-cols-2">{text('publicKey', 'Public key')}{text('shortId', 'Short ID')}</div> : null}
    </> : null}
  </div>
}
