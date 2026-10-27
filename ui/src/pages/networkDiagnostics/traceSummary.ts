import type { NetworkTraceStep } from '../../lib/networkDiagnosticsTypes'

type SummaryRow = [label: string, value: string]

const isAddress = (value: string) => /^(?:\d{1,3}\.){3}\d{1,3}$/.test(value) || (value.includes(':') && /^[a-f\d:]+$/i.test(value))

export function traceSummary(step: NetworkTraceStep, target: string | undefined, zh: boolean): SummaryRow[] {
  const rows: SummaryRow[] = []
  if (step.phase === 'frontend' || step.phase === 'dns') {
    if (target) {
      try {
        rows.push([zh ? '查询目标' : 'Lookup target', new URL(target).hostname])
      } catch { /* The request may still be awaiting target validation. */ }
    }
    if (step.source === 'frontend' && step.phase === 'frontend') {
      for (const type of ['A', 'AAAA']) {
        const records = step.evidence.filter((item) => item.startsWith(`${type}: `)).map((item) => item.slice(type.length + 2))
        if (!records.length) continue
        const response = records.map((item) => item.match(/^upstream=(.*?) rcode=(\d+) decision=(.*)$/)).filter((item) => item !== null).at(-1)
        const addresses = records.filter(isAddress)
        if (response) {
          rows.push([`${type} · ${zh ? '返回来源' : 'Response source'}`, response[1] || (zh ? '未提供' : 'Not reported')])
          rows.push([`${type} · ${zh ? '解析结果' : 'Result'}`, [addresses.join(' · ') || (zh ? '无地址返回' : 'No addresses returned'), `rcode ${response[2]}`, response[3]].filter(Boolean).join(' · ')])
        } else {
          const upstream = records.map((item) => item.match(/^query selected upstream: (.*)$/)).filter((item) => item !== null).at(-1)
          if (upstream) rows.push([`${type} · ${zh ? '已选择上游' : 'Selected upstream'}`, upstream[1]])
          rows.push([`${type} · ${zh ? '最新记录' : 'Latest evidence'}`, records.at(-1)!])
        }
      }
    } else if (step.phase === 'dns') {
      const addresses = step.evidence.filter(isAddress)
      if (addresses.length) rows.push([zh ? '返回地址' : 'Returned addresses', addresses.join(' · ')])
      if (step.source === 'socket' && target) {
        try {
          const host = new URL(target).hostname.replace(/^\[|\]$/g, '')
          if (!isAddress(host)) rows.push([zh ? '解析入口' : 'Resolver', zh ? '系统解析器 · 上游未观测' : 'System resolver · upstream not observed'])
        } catch { /* No resolver identity is inferred from an invalid target. */ }
      }
    }
  }
  if (step.phase === 'connection') {
    const destination = step.evidence.find((item) => item.startsWith('destination='))
    if (destination) rows.push([zh ? '连接目标' : 'Connection target', destination.slice('destination='.length)])
  }
  if (step.rule) rows.push([zh ? '命中规则' : 'Matched rule', [step.rule.type, step.rule.payload].filter(Boolean).join(' · ')])
  if (step.chains?.length) rows.push([zh ? '代理链 · 内核返回顺序' : 'Proxy chain · API order', step.chains.join(' → ')])
  return rows
}
