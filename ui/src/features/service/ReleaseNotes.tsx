import type { ReactNode } from 'react'

type Block =
  | { kind: 'heading'; text: string }
  | { kind: 'paragraph'; text: string }
  | { kind: 'list'; ordered: boolean; items: string[] }

type Release = { version: string; published_at: string; notes: string }

export function ReleaseNotes({ releases, locale }: { releases: Release[]; locale: string }) {
  return <div className="max-h-80 overflow-auto rounded-md border border-[var(--border)] bg-[var(--surface)] px-3 text-sm leading-6">
    {releases.map((release, index) => <section key={release.version} className={index ? 'border-t border-[var(--border)] py-3' : 'py-3'}>
      <div className="mb-2 flex flex-wrap items-baseline justify-between gap-2">
        <h4 className="font-mono text-sm font-semibold">v{release.version.replace(/^v/, '')}</h4>
        {release.published_at ? <time className="text-xs text-[var(--muted)]">{new Date(release.published_at).toLocaleString(locale)}</time> : null}
      </div>
      <MarkdownBlocks markdown={selectLocalizedReleaseNotes(release.notes, locale)} />
    </section>)}
  </div>
}

function MarkdownBlocks({ markdown }: { markdown: string }) {
  const blocks = parseBlocks(markdown)
  return <div className="space-y-3">
    {blocks.map((block, index) => {
      if (block.kind === 'heading') return <h5 key={index} className="font-semibold text-[var(--text)]">{renderInline(block.text)}</h5>
      if (block.kind === 'paragraph') return <p key={index}>{renderInline(block.text)}</p>
      const List = block.ordered ? 'ol' : 'ul'
      return <List key={index} className={`${block.ordered ? 'list-decimal' : 'list-disc'} space-y-1 pl-5`}>
        {block.items.map((item, itemIndex) => <li key={itemIndex}>{renderInline(item)}</li>)}
      </List>
    })}
  </div>
}

export function selectLocalizedReleaseNotes(markdown: string, locale: string) {
  const lines = markdown.replaceAll('\r\n', '\n').split('\n')
  const heading = locale === 'zh-CN' ? '简体中文' : 'English'
  const start = lines.findIndex((line) => line.trim() === `## ${heading}`)
  if (start < 0) return markdown.trim()
  const nextSection = lines.findIndex((line, index) => index > start && /^##\s+\S/.test(line.trim()))
  return lines.slice(start + 1, nextSection < 0 ? undefined : nextSection).join('\n').trim()
}

function parseBlocks(markdown: string): Block[] {
  const lines = markdown.replaceAll('\r\n', '\n').split('\n')
  const blocks: Block[] = []
  for (let index = 0; index < lines.length;) {
    const line = lines[index].trim()
    if (!line) { index += 1; continue }
    const heading = line.match(/^#{1,6}\s+(.+)$/)
    if (heading) { blocks.push({ kind: 'heading', text: heading[1] }); index += 1; continue }
    const unordered = line.match(/^[-*]\s+(.+)$/)
    const ordered = line.match(/^\d+\.\s+(.+)$/)
    if (unordered || ordered) {
      const isOrdered = Boolean(ordered)
      const items: string[] = []
      while (index < lines.length) {
        const item = lines[index].trim().match(isOrdered ? /^\d+\.\s+(.+)$/ : /^[-*]\s+(.+)$/)
        if (!item) break
        items.push(item[1])
        index += 1
      }
      blocks.push({ kind: 'list', ordered: isOrdered, items })
      continue
    }
    const paragraph = [line]
    index += 1
    while (index < lines.length && lines[index].trim() && !isBlockStart(lines[index].trim())) {
      paragraph.push(lines[index].trim())
      index += 1
    }
    blocks.push({ kind: 'paragraph', text: paragraph.join(' ') })
  }
  return blocks
}

function isBlockStart(line: string) {
  return /^#{1,6}\s+|^[-*]\s+|^\d+\.\s+/.test(line)
}

function renderInline(text: string): ReactNode[] {
  const tokens = text.split(/(\*\*[^*\n]+\*\*|`[^`\n]+`|\[[^\]\n]+\]\(https?:\/\/[^)\s]+\))/g).filter(Boolean)
  return tokens.map((token, index) => {
    if (token.startsWith('**') && token.endsWith('**')) return <strong key={index}>{token.slice(2, -2)}</strong>
    if (token.startsWith('`') && token.endsWith('`')) return <code key={index} className="rounded bg-[var(--surface-hover)] px-1 py-0.5 font-mono text-xs">{token.slice(1, -1)}</code>
    const link = token.match(/^\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)$/)
    if (link) return <a key={index} className="text-emerald-700 underline underline-offset-2 dark:text-emerald-400" href={link[2]} target="_blank" rel="noreferrer">{link[1]}</a>
    return token
  })
}
