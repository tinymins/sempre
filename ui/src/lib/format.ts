export function formatBytes(value = 0, suffix = '') {
  if (!Number.isFinite(value) || value <= 0) return `0 B${suffix}`
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']
  const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1)
  const scaled = value / 1024 ** index
  return `${scaled >= 100 || index === 0 ? scaled.toFixed(0) : scaled.toFixed(1)} ${units[index]}${suffix}`
}

export function formatDate(value?: string) {
  if (!value) return '-'
  const date = new Date(value)
  return Number.isNaN(date.valueOf()) ? '-' : date.toLocaleString()
}

export function compactHash(value?: string) {
  if (!value) return '-'
  return `${value.slice(0, 8)}...${value.slice(-6)}`
}
