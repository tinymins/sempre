import { Button } from '@acme/components'
import { useSortable } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { GripVertical } from 'lucide-react'
import type { ReactNode } from 'react'
import { useEditorI18n } from './i18n'

export function SortableSource({ id, index, disabled, children }: { id: string; index: number; disabled?: boolean; children: (handle: ReactNode) => ReactNode }) {
  const { t, number } = useEditorI18n()
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id, disabled })
  return <div ref={setNodeRef} style={{ transform: CSS.Transform.toString(transform), transition, opacity: isDragging ? 0.6 : 1 }} className="space-y-2 rounded-lg border border-[var(--border)] bg-[var(--surface)] p-3">
    {children(<Button size="small" disabled={disabled} icon={<GripVertical size={14} />} className="touch-none" aria-label={t('source.reorder', { index: number(index + 1) })} {...attributes} {...listeners} />)}
  </div>
}
