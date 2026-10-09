import { useId, useState } from 'react'
import { CircleAlert } from 'lucide-react'
import { Button, Input, Tooltip } from '@acme/components'

export function DnsUpstreamsInput({ upstreams, onChange, saving, zh }: { saving: boolean; upstreams: string[]; onChange: (value: string[]) => void; zh: boolean }) {
  const id = useId()
  const saved = upstreams.join(', ')
  const [input, setInput] = useState<{ text: string; saved: string } | null>(null)
  const text = input?.saved === saved ? input.text : saved
  const warning = zh
    ? '不建议修改默认 DoT 上游。使用 UDP/TCP 53 端口的上游容易被本机其他软件再次劫持，可能造成循环查询、解析超时。出现问题时请清空输入框并移开焦点，恢复默认 DoT 配置。'
    : 'Changing the default DoT upstreams is not recommended. UDP/TCP port 53 can be intercepted again by local software, causing DNS loops and timeouts. Clear this field and move focus away to restore the default DoT configuration.'
  return <div className="space-y-2 rounded-md border border-[var(--border)] p-4">
    <label htmlFor={id} className="block text-sm font-medium">{zh ? '前置 DNS 上游' : 'DNS frontend upstreams'}</label>
    <div className="flex items-center gap-2">
      <Input id={id} className="min-w-0 flex-1" value={text} disabled={saving} placeholder="tls://223.6.6.6:853?server_name=dns.alidns.com" onChange={(event) => setInput({ text: event.target.value, saved })} onBlur={() => {
        const next = text.split(/[,\n]/).map((value) => value.trim()).filter(Boolean)
        if (JSON.stringify(next) !== JSON.stringify(upstreams)) onChange(next)
      }} onKeyDown={(event) => { if (event.key === 'Enter') event.currentTarget.blur() }} />
      <Tooltip title={warning}>
        <Button variant="text" className="shrink-0 text-amber-600 dark:text-amber-400" aria-label={zh ? '修改上游的风险' : 'Upstream configuration risks'} icon={<CircleAlert size={18} />} />
      </Tooltip>
    </div>
    <div className="text-xs text-[var(--muted)]">{zh ? '支持裸 IP 或主机名（默认 UDP 53），以及 tls://、tcp://、udp://；多个地址用逗号分隔，按顺序尝试。移开焦点自动保存；留空恢复默认 DoT。用于国内域名和自定义直连规则。' : 'Supports bare IPs or hostnames (UDP 53 by default), plus tls://, tcp:// and udp://. Separate upstreams with commas; they are tried in order. Saved automatically when focus leaves the field. Leave empty to restore default DoT. Used for domestic domains and custom direct rules.'}</div>
  </div>
}
