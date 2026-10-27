import { Collapse } from '@acme/components'
import type { LeakMode } from './useLeakDetection'

type Note = { title: string; body: string }

function notes(mode: LeakMode, zh: boolean): Note[] {
  const unknown = { title: zh ? '“未知”不代表没有泄漏' : '“Unknown” does not mean leak-free', body: zh ? '结论仅覆盖本次检测时段、目标进程和已执行的探针。核心快照、套接字和物理接口证据的范围不同；缺少证据时保留未知。' : 'Results cover only this run, its target processes and executed probes. Core snapshots, sockets and physical interface evidence cover different scopes; missing evidence remains unknown.' }
  const connections = { title: zh ? '连接证据的含义' : 'What connection evidence means', body: zh ? '核心代理路由表示连接命中代理选路，不等于已证明物理出口。DIRECT 表示核心选择直连，需按目标与策略核实；未归属连接不能直接判定绕过。物理接口上的连接需核对是否为显式代理端点。' : 'A proxy route proves a core routing decision, not the physical egress. DIRECT means the core selected direct routing and needs policy review. Unattributed connections do not prove bypass. Physical interface connections need checking against explicit proxy endpoints.' }
  if (mode === 'quick') return [unknown,
    { title: zh ? '本机探针' : 'Local probes', body: zh ? 'Sempre 启动独立临时进程检查 DNS 与 HTTPS IPv4 / IPv6，结果只代表探针，不代表 Codex / Claude Code。出口与预期不同也可能是代理出口轮换导致。' : 'Sempre starts independent temporary processes for DNS and HTTPS IPv4 / IPv6. Results represent the probes, not Codex / Claude Code. A differing egress can also come from proxy egress rotation.' },
    { title: zh ? '浏览器探针覆盖范围' : 'Browser probe coverage', body: zh ? '检测 HTTPS 出口和 WebRTC 候选地址；局域网、保留地址和 mDNS 不作为公网 IP 泄漏。浏览器 DNS / DoH 路径、HTTP/3、WebSocket 及扩展流量不在覆盖范围内。浏览器可能与 Sempre 不在同一设备。' : 'Covers HTTPS egress and WebRTC candidates; LAN, reserved and mDNS addresses are not public IP leaks. Browser DNS / DoH paths, HTTP/3, WebSocket and extension traffic are not covered. The browser may be on a different device from Sempre.' },
    connections,
    { title: zh ? '外部端点与隐私' : 'External endpoints and privacy', body: zh ? '探针会访问 api.ipify.org、api6.ipify.org 与 stun.l.google.com:19302，服务方可看到来源地址；不发送 Sempre 凭据，也不修改 TUN、DNS 或代理配置。导出的 JSON 含 IP、进程与连接信息。' : 'Probes contact api.ipify.org, api6.ipify.org and stun.l.google.com:19302, which can see the source address. Sempre credentials are not sent and TUN, DNS or proxy settings are not changed. Exported JSON contains IP, process and connection details.' },
  ]
  return [unknown, connections,
    { title: zh ? 'DNS 路径' : 'DNS paths', body: zh ? '公共 DNS 地址、解析器所在国家或 FakeIP 地址不能单独证明泄漏。系统解析器可能由多个应用共享；加密 DNS 的内容通常不可见，443 端口也不能单独证明 DoH。' : 'A public resolver address, resolver country or FakeIP address does not alone prove a leak. System resolvers may be shared. Encrypted DNS contents are usually opaque, and port 443 alone does not prove DoH.' },
    { title: zh ? '设备接口' : 'Device interfaces', body: zh ? 'TUN 或网关 LAN 中出现目标 IP 本身不代表泄漏，需结合接口方向、进程套接字和核心选路关联；物理接口上的代理服务器连接是正常隧道流量。' : 'A destination on TUN or gateway LAN does not itself indicate a leak; interface direction, process sockets and core routing must be correlated. Proxy-server connections on physical interfaces are normal tunnel traffic.' },
    { title: zh ? '覆盖缺口' : 'Coverage gaps', body: zh ? '未捕获的短连接、独立启动的 MCP 进程和系统共享 DNS 可能未覆盖。网关模式无法看到客户端进程，绕过此网关的流量也不可见。' : 'Short-lived connections, independently started MCP processes and shared system DNS may remain uncovered. Gateway mode cannot see client processes or traffic that bypasses this gateway.' },
  ]
}

export function LeakNotes({ mode, zh }: { mode: LeakMode; zh: boolean }) {
  return <Collapse size="small" ghost items={[{ key: 'notes', label: zh ? '如何解读结果' : 'How to read the results', children: <dl className="min-w-0 space-y-3 text-xs">
    {notes(mode, zh).map((note) => <div key={note.title}><dt className="font-medium">{note.title}</dt><dd className="mt-1 break-words text-[var(--muted)]">{note.body}</dd></div>)}
  </dl> }]} />
}
