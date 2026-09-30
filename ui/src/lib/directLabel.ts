const DIRECT_LABEL = '🚀 直接连接'

export function isBuiltinDirect(name: string): boolean {
  return name === 'direct' || name === 'DIRECT' || name === DIRECT_LABEL
}

export function directLabel(name: string): string {
  return isBuiltinDirect(name) ? DIRECT_LABEL : name
}
