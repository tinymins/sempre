import { Button } from '@acme/components'
import { useSortable } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { GripVertical } from 'lucide-react'
import type { ReactNode } from 'react'
import { useEditorI18n } from './i18n'
import { useEditorLayout } from './layout'

export function SortableSource({ id, index, disabled, enabled, type, children }: { id: string; index: number; disabled?: boolean; enabled?: boolean; type: 'url' | 'raw'; children: (handle: ReactNode) => ReactNode }) {
  const { t, number } = useEditorI18n()
  const page = useEditorLayout() === 'page'
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id, disabled: disabled || (page && type === 'raw') })
  return <div ref={setNodeRef} style={{ transform: CSS.Transform.toString(transform), transition, opacity: isDragging ? page ? 0.5 : 0.6 : 1 }} className={page ? type === 'raw' ? 'rounded-lg border border-gray-200 p-3 dark:border-gray-700' : `rounded-lg border p-3 transition-colors ${enabled ? 'border-gray-200 bg-white dark:border-gray-600 dark:bg-[#1a1a1a]' : 'border-dashed border-gray-300 bg-gray-50 opacity-60 dark:border-gray-700 dark:bg-[#111]'}` : 'space-y-2 rounded-lg border border-[var(--border)] bg-[var(--surface)] p-3'}>
    {children(<Button variant={page ? 'unstyled' : undefined} size="small" disabled={disabled} icon={<GripVertical size={14} />} className={page ? 'touch-none cursor-grab text-gray-400 hover:text-gray-600 dark:hover:text-gray-300' : 'touch-none'} aria-label={t('source.reorder', { index: number(index + 1) })} {...attributes} {...listeners} />)}
  </div>
}
