import { Button, Input, Modal } from '@acme/components'
import { useEffect, useMemo, useRef, useState } from 'react'
import type { DraftDebugResult } from './diagnostic-types'
import { diagnosticText } from './diagnostic-locale'
import { useI18n } from '../i18n/provider'

type Section = { title: string; lines: string[] }
type Match = { section: number; line: number }

function highlight(line: string, query: string) {
  if (!query) return line
  const pieces = []
  const lower = line.toLowerCase()
  const token = query.toLowerCase()
  let start = 0
  let index = lower.indexOf(token)
  while (index >= 0) {
    if (index > start) pieces.push(<span key={`text-${start}`}>{line.slice(start, index)}</span>)
    pieces.push(<mark key={`match-${index}`} className="rounded bg-yellow-300 text-black">{line.slice(index, index + query.length)}</mark>)
    start = index + query.length
    index = lower.indexOf(token, start)
  }
  if (start < line.length) pieces.push(<span key={`tail-${start}`}>{line.slice(start)}</span>)
  return pieces
}

export function DiagnosticSearch({ result, open, onClose }: { result: DraftDebugResult; open: boolean; onClose: () => void }) {
  const { locale, number } = useI18n()
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const activeRef = useRef<HTMLDivElement>(null)
  const sections = useMemo<Section[]>(() => [
    ...(result.content !== undefined ? [{ title: diagnosticText(locale, 'finalConfig'), lines: result.content.split('\n') }] : []),
    ...(result.ruleSamples ?? []).map((sample) => ({ title: sample.section, lines: sample.lines })),
  ], [result.content, result.ruleSamples, locale])
  const matches = useMemo(() => {
    const needle = query.trim().toLowerCase()
    if (needle.length < 2) return []
    const next: Match[] = []
    sections.forEach((section, sectionIndex) => section.lines.forEach((line, lineIndex) => {
      if (line.toLowerCase().includes(needle)) next.push({ section: sectionIndex, line: lineIndex })
    }))
    return next
  }, [query, sections])
  useEffect(() => {
    if (open && matches.length) activeRef.current?.scrollIntoView({ block: 'center', behavior: 'smooth' })
  }, [open, active, matches])
  const selected = matches[active]
  return <Modal open={open} title={diagnosticText(locale, 'searchTitle')} size="large" footer={null} onCancel={onClose}>
    <div className="space-y-4">
        <Input value={query} onChange={(event) => { setQuery(event.target.value); setActive(0) }} placeholder={diagnosticText(locale, 'searchHint')} />
      <div className="flex items-center gap-2 text-sm">
        <span>{number(matches.length)} {diagnosticText(locale, 'searchCount')}</span>
        <Button size="small" disabled={!matches.length} onClick={() => setActive((index) => (index - 1 + matches.length) % matches.length)}>{diagnosticText(locale, 'prev')}</Button>
        <Button size="small" disabled={!matches.length} onClick={() => setActive((index) => (index + 1) % matches.length)}>{diagnosticText(locale, 'next')}</Button>
        {selected ? <span>{number(active + 1)} / {number(matches.length)}</span> : null}
      </div>
      {matches.length ? <div className="max-h-[60vh] space-y-4 overflow-auto">
        {sections.map((section, sectionIndex) => {
          const lines = new Set<number>()
          matches.filter((match) => match.section === sectionIndex).forEach(({ line }) => {
            for (let index = Math.max(0, line - 2); index <= Math.min(section.lines.length - 1, line + 2); index++) lines.add(index)
          })
          if (!lines.size) return null
          return <section key={`${section.title}-${sectionIndex}`} className="space-y-2">
            <h3 className="text-sm font-semibold">{section.title}</h3>
            <div className="overflow-x-auto rounded border border-[var(--border)] bg-[var(--surface)] p-2 font-mono text-xs">
              {[...lines].sort((a, b) => a - b).map((line) => {
                const matchIndex = matches.findIndex((item) => item.section === sectionIndex && item.line === line)
                const focused = matchIndex === active
                return <div key={line} ref={focused ? activeRef : undefined} className={`flex gap-3 whitespace-pre ${focused ? 'bg-blue-100 dark:bg-blue-900/30' : ''}`}>
                  <span className="select-none text-[var(--muted)]">{number(line + 1)}</span><span>{highlight(section.lines[line], query.trim())}</span>
                </div>
              })}
            </div>
          </section>
        })}
      </div> : query.trim().length >= 2 ? <p className="text-sm text-[var(--muted)]">{diagnosticText(locale, 'searchEmpty')}</p> : null}
    </div>
  </Modal>
}
