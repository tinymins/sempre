import type { NetworkTraceStep } from '../../lib/networkDiagnosticsTypes'

export function traceTitle(step: NetworkTraceStep, zh: boolean) {
  const titles: Record<NetworkTraceStep['phase'], [string, string, string]> = zh ? {
    frontend: ['前置正在判断 DNS 规则', '前置完成 DNS 解析', '前置 DNS 解析失败'],
    dns: ['开始访问目标 · 解析地址', '目标地址已确认', '目标地址解析失败'],
    connection: ['正在建立连接', 'TCP 连接已建立', '连接失败'],
    core: ['等待内核连接记录', '捕获内核规则与代理链', '内核采集失败'],
    tls: ['正在建立 TLS 连接', 'TLS 握手已完成', 'TLS 握手失败'],
    request: ['正在发送请求', '请求已写入', '发送请求失败'],
    http: ['等待目标响应', '目标响应已收到', '等待目标响应失败'],
  } : {
    frontend: ['Evaluating frontend DNS rules', 'Frontend DNS resolved', 'Frontend DNS failed'],
    dns: ['Starting request · resolving target', 'Target address confirmed', 'Target resolution failed'],
    connection: ['Connecting to target', 'TCP connection established', 'Connection failed'],
    core: ['Waiting for core connection record', 'Core rule and chain captured', 'Core capture failed'],
    tls: ['Establishing TLS', 'TLS handshake completed', 'TLS handshake failed'],
    request: ['Sending request', 'Request written', 'Request write failed'],
    http: ['Waiting for target response', 'Target response received', 'Target response failed'],
  }
  if (step.status === 'unknown') return zh ? (step.phase === 'core' ? '内核链路未能确认' : '前置处理未能确认') : 'Evidence unavailable'
  return titles[step.phase][step.status === 'running' ? 0 : step.status === 'passed' ? 1 : 2]
}

// These are display translations of observed records, never additional decisions.
export function decisionRows(evidence: string[], zh: boolean): [string, string][] {
  return evidence.flatMap((raw): [string, string][] => {
    const query = raw.match(/^(A|AAAA): (.*)$/)
    const text = query?.[2] ?? raw
    const prefix = query ? `${query[1]} · ` : ''
    const rule = text.match(/^rule-set:(.+) (not matched|matched)$/)
    if (rule) return [[`${prefix}${zh ? '规则集' : 'Rule set'} · ${rule[1]}`, rule[2] === 'matched' ? (zh ? '命中' : 'Matched') : (zh ? '未命中 → 继续' : 'Not matched → continue')]]
    const upstream = text.match(/^(?:query selected upstream: |Predicted DNS upstream: )(.*)$/)
    if (upstream) return [[`${prefix}${zh ? 'DNS 上游' : 'DNS upstream'}`, upstream[1]]]
    const rewrite = text.match(/^(?:A )?rewrite:(.*)$/)
    if (rewrite) return [[`${prefix}${zh ? 'DNS 重写' : 'DNS rewrite'}`, rewrite[1].trim() === 'no match' ? (zh ? '未命中 → 继续' : 'Not matched → continue') : rewrite[1].replace('matched', zh ? '命中' : 'matched')]]
    const known: Record<string, [string, string]> = {
      'HTTPS rejection: not applied': ['HTTPS 查询拦截', '未应用 → 继续'],
      'HTTPS rejection: matched': ['HTTPS 查询拦截', '命中'],
      'default: query remote upstream': ['默认策略', '查询远程 DNS'],
      'response-first: query local upstream': ['响应优先策略', '查询本地 DNS'],
      'local response: domestic address matched': ['本地 DNS 响应', '命中国内地址'],
      'local response not accepted: query remote upstream': ['本地 DNS 响应', '未采用 → 查询远程 DNS'],
    }
    if (known[text]) return [zh ? [`${prefix}${known[text][0]}`, known[text][1]] : [prefix + 'Observed decision', text]]
    const endpoint = text.match(/^(source|destination|connection_id)=(.*)$/)
    if (endpoint) return [[zh ? ({ source: '本地端点', destination: '目标端点', connection_id: '内核连接 ID' }[endpoint[1]] ?? endpoint[1]) : endpoint[1], endpoint[2]]]
    if (/^(?:\d{1,3}\.){3}\d{1,3}$/.test(text) || /^[a-f\d:]+$/i.test(text)) return [[`${prefix}${zh ? '解析地址' : 'Resolved address'}`, text]]
    const response = text.match(/^upstream=(.*?) rcode=(\d+) decision=(.*)$/)
    if (response) return [[`${prefix}${zh ? 'DNS 返回' : 'DNS response'}`, `${response[1]} · rcode ${response[2]} · ${response[3]}`]]
    return []
  })
}
