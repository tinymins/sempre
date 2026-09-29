import { randomUuid } from '@acme/components'

interface SourceFields {
  id: string
  enabled: boolean
  prefix: string
  remark: string
  cacheTtlMinutes?: number
  fetchUa?: string
  fetchMode?: 'auto' | 'domestic-direct'
}

export type EditorSource = SourceFields & (
  { type: 'url'; url: string; content?: never } |
  { type: 'raw'; content: string; url?: never }
)

export function createSource(type: EditorSource['type'] = 'url'): EditorSource {
  const common = { id: randomUuid(), enabled: true, prefix: '', remark: '' }
  return type === 'raw' ? { ...common, type, content: '' } : { ...common, type, url: '', fetchMode: 'auto' }
}

export function normalizeSource(value: Partial<EditorSource>): EditorSource {
  const common = { ...value, id: value.id || randomUuid(), enabled: value.enabled !== false, prefix: value.prefix ?? '', remark: value.remark ?? '' }
  return value.type === 'raw' ? { ...common, type: 'raw', content: value.content ?? '', url: undefined } : { ...common, type: 'url', url: value.url ?? '', content: undefined }
}

export function sourceText(source: EditorSource): string {
  return source.type === 'raw' ? source.content : source.url
}

export function moveSource(sources: EditorSource[], from: number, to: number): EditorSource[] {
  if (from < 0 || to < 0 || from >= sources.length || to >= sources.length) return sources
  const next = [...sources]
  const [source] = next.splice(from, 1)
  next.splice(to, 0, source)
  return next
}
