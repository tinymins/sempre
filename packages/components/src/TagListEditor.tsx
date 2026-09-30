import { Plus, X } from "lucide-react";
import { useState } from "react";
import { Button } from "./Button";
import { Input } from "./Input";

export interface TagListEdit {
  index: number;
  /** Undefined removes the tag; an index equal to the list length appends it. */
  value: string | undefined;
}

export interface TagListEditorProps {
  value: string[];
  /** The edit identifies the changed item for consumers with structured storage. */
  onChange?: (value: string[], edit: TagListEdit) => void;
  readOnly?: boolean;
  disabled?: boolean;
  addLabel: string;
  removeLabel: (tag: string) => string;
  emptyLabel?: string;
}

export function TagListEditor({
  value,
  onChange,
  readOnly = false,
  disabled = false,
  addLabel,
  removeLabel,
  emptyLabel,
}: TagListEditorProps) {
  const [newTag, setNewTag] = useState("");
  const [isAdding, setIsAdding] = useState(false);
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [editingTag, setEditingTag] = useState("");
  const edit = (index: number, next: string | undefined) => {
    if (readOnly || disabled) return;
    const tags = [...value];
    if (next === undefined) tags.splice(index, 1);
    else tags[index] = next;
    onChange?.(tags, { index, value: next });
  };
  const add = () => {
    const tag = newTag.trim();
    if (tag && !value.includes(tag)) edit(value.length, tag);
    setNewTag("");
    setIsAdding(false);
  };
  const finishEdit = () => {
    if (editingIndex === null) return;
    const next = editingTag.trim();
    if (next && next !== value[editingIndex] && !value.some((tag, index) => index !== editingIndex && tag === next)) {
      edit(editingIndex, next);
    }
    setEditingIndex(null);
  };

  return (
    <div className="flex flex-wrap items-center gap-2">
      {value.map((tag, index) => (
        <span key={`${index}-${tag}`} className={`inline-flex min-h-7 max-w-full items-center gap-1 rounded-md border border-[var(--border)] bg-[var(--input-bg)] px-2.5 py-1 text-sm leading-5 ${readOnly ? "text-[var(--text-muted)]" : "text-[var(--text-primary)]"}`}>
          {editingIndex === index ? (
            <Input
              variant="inline"
              autoFocus
              disabled={disabled}
              value={editingTag}
              style={{ width: `${Math.max(editingTag.length, 3)}ch`, minWidth: 40 }}
              onChange={(event) => setEditingTag(event.target.value)}
              onBlur={finishEdit}
              onKeyDown={(event) => {
                if (event.key === "Enter") { event.preventDefault(); finishEdit(); }
                if (event.key === "Escape") { event.stopPropagation(); setEditingIndex(null); }
              }}
            />
          ) : readOnly ? (
            <span className="min-w-0 [overflow-wrap:anywhere]">{tag}</span>
          ) : (
            <Button variant="unstyled" disabled={disabled} className="min-w-0 text-left [overflow-wrap:anywhere]" onClick={() => { setEditingIndex(index); setEditingTag(tag); }}>
              {tag}
            </Button>
          )}
          {!readOnly ? (
            <Button
              variant="unstyled"
              disabled={disabled}
              className="size-4 shrink-0 rounded-sm text-[var(--text-muted)] transition-colors hover:text-red-500"
              icon={<X size={13} />}
              aria-label={removeLabel(tag)}
              onClick={() => edit(index, undefined)}
            />
          ) : null}
        </span>
      ))}
      {!readOnly ? isAdding ? (
        <span className="inline-flex max-w-full items-center rounded-md border border-[var(--accent)] bg-[var(--input-bg)] px-2.5 py-1 text-sm leading-5">
          <Input
            variant="inline"
            autoFocus
            value={newTag}
            disabled={disabled}
            aria-label={addLabel}
            placeholder={addLabel}
            onChange={(event) => setNewTag(event.target.value)}
            onBlur={add}
            onKeyDown={(event) => {
              if (event.key === "Enter") { event.preventDefault(); add(); }
              if (event.key === "Escape") { event.stopPropagation(); setNewTag(""); setIsAdding(false); }
            }}
            style={{ width: "18ch", minWidth: 80 }}
          />
        </span>
      ) : (
        <Button variant="dashed" className="h-8 shrink-0 px-2.5 text-[var(--text-muted)]" icon={<Plus size={14} />} aria-label={addLabel} title={addLabel} disabled={disabled} onClick={() => setIsAdding(true)} />
      ) : null}
      {value.length === 0 && !disabled && readOnly ? <span className="text-sm text-[var(--text-muted)]">{emptyLabel}</span> : null}
    </div>
  );
}
