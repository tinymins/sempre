import { useId, useState } from 'react'
import { CircleAlert } from 'lucide-react'
import { Button, Input, Tooltip } from '@acme/components'

export function DnsUpstreamsInput({ upstreams, onChange, zh }: { upstreams: string[]; onChange: (value: string[]) => void; zh: boolean }) {
  const id = useId()
  const [text, setText] = useState(() => upstreams.join(', '))
  const warning = zh
    ? '不建议修改默认 DoT 上游。使用 UDP/TCP 53 端口的上游容易被本机其他软件再次劫持，可能造成循环查询、解析超时。出现问题时请清空输入框并保存，恢复默认 DoT 配置。'
    : 'Changing the default DoT upstreams is not recommended. UDP/TCP port 53 can be intercepted again by local software, causing DNS loops and timeouts. Clear this field and save to restore the default DoT configuration.'
  return <div className="space-y-2 rounded-md border border-[var(--border)] p-4">
    <label htmlFor={id} className="block text-sm font-medium">{zh ? '前置 DNS 上游' : 'DNS frontend upstreams'}</label>
    <div className="flex items-center gap-2">
      <Input id={id} className="min-w-0 flex-1" value={text} placeholder="tls://223.6.6.6:853?server_name=dns.alidns.com" onChange={(event) => {
        setText(event.target.value)
        onChange(event.target.value.split(/[,\n]/).map((value) => value.trim()).filter(Boolean))
      }} />
      <Tooltip title={warning}>
        <Button variant="text" className="shrink-0 text-amber-600 dark:text-amber-400" aria-label={zh ? '修改上游的风险' : 'Upstream configuration risks'} icon={<CircleAlert size={18} />} />
      </Tooltip>
    </div>
    <div className="text-xs text-[var(--muted)]">{zh ? '支持 tls://、tcp://、udp://；多个地址用逗号分隔，按顺序尝试。留空保存恢复默认 DoT。用于国内域名和自定义直连规则。' : 'Supports tls://, tcp:// and udp://. Separate upstreams with commas; they are tried in order. Leave empty and save to restore default DoT. Used for domestic domains and custom direct rules.'}</div>
  </div>
}
