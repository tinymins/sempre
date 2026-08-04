declare module 'lucide' {
  type IconNode = readonly [string, Record<string, string>][]
  export function createIcons(options: {
    icons: Record<string, IconNode>
    attrs?: Record<string, string>
  }): void
  export const Activity: IconNode
  export const ArrowDown: IconNode
  export const ArrowRight: IconNode
  export const ArrowUpRight: IconNode
  export const BadgeCheck: IconNode
  export const Box: IconNode
  export const Check: IconNode
  export const Command: IconNode
  export const Copy: IconNode
  export const CircleCheck: IconNode
  export const FileCheck2: IconNode
  export const FileJson2: IconNode
  export const GitFork: IconNode
  export const Languages: IconNode
  export const Monitor: IconNode
  export const MonitorDot: IconNode
  export const Moon: IconNode
  export const Server: IconNode
  export const ServerCog: IconNode
  export const ShieldCheck: IconNode
  export const Sun: IconNode
  export const Terminal: IconNode
}
