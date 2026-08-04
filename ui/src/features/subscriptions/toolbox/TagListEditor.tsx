import { Plus, X } from "lucide-react";
import { useRef, useState } from "react";

interface TagListEditorProps {
  value?: string[];
  onChange?: (value: string[]) => void;
  readOnly?: boolean;
  placeholder?: string;
}

const TagListEditor = ({
  value = [],
  onChange,
  readOnly,
  placeholder,
}: TagListEditorProps) => {
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [editValue, setEditValue] = useState("");
  const [isAdding, setIsAdding] = useState(false);
  const [addValue, setAddValue] = useState("");
  const addInputRef = useRef<HTMLInputElement>(null);
  const editInputRef = useRef<HTMLInputElement>(null);

  const handleDelete = (index: number) => {
    if (readOnly) return;
    const next = value.filter((_, i) => i !== index);
    onChange?.(next);
  };

  const handleStartEdit = (index: number) => {
    if (readOnly) return;
    setEditingIndex(index);
    setEditValue(value[index]);
    setTimeout(() => editInputRef.current?.focus(), 0);
  };

  const handleFinishEdit = () => {
    if (editingIndex === null) return;
    const trimmed = editValue.trim();
    if (trimmed && trimmed !== value[editingIndex]) {
      const next = [...value];
      next[editingIndex] = trimmed;
      onChange?.(next);
    }
    setEditingIndex(null);
    setEditValue("");
  };

  const handleStartAdd = () => {
    setIsAdding(true);
    setAddValue("");
    setTimeout(() => addInputRef.current?.focus(), 0);
  };

  const handleFinishAdd = () => {
    const trimmed = addValue.trim();
    if (trimmed && !value.includes(trimmed)) {
      onChange?.([...value, trimmed]);
    }
    setIsAdding(false);
    setAddValue("");
  };

  return (
    <div className="flex flex-wrap items-center gap-2">
      {value.map((tag, index) => (
        <span
          key={tag}
          className={`inline-flex items-center gap-1 px-2.5 py-1 rounded-md text-sm border transition-colors ${
            readOnly
              ? "bg-gray-100 dark:bg-zinc-800 border-gray-200 dark:border-zinc-700 text-gray-500 dark:text-zinc-400"
              : "bg-gray-50 dark:bg-zinc-800 border-gray-200 dark:border-zinc-600 text-gray-800 dark:text-zinc-200"
          }`}
        >
          {editingIndex === index ? (
            <input
              ref={editInputRef}
              value={editValue}
              onChange={(e) => setEditValue(e.target.value)}
              onBlur={handleFinishEdit}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleFinishEdit();
                if (e.key === "Escape") {
                  setEditingIndex(null);
                  setEditValue("");
                }
              }}
              className="bg-transparent outline-none border-none p-0 text-sm w-auto min-w-[40px]"
              style={{ width: `${Math.max(editValue.length, 3)}ch` }}
            />
          ) : (
            <span
              className={readOnly ? "" : "cursor-pointer"}
              onClick={() => handleStartEdit(index)}
            >
              {tag}
            </span>
          )}
          {!readOnly && (
            <button
              type="button"
              className="cursor-pointer text-gray-400 hover:text-red-500 dark:text-zinc-500 dark:hover:text-red-400 transition-colors"
              onClick={() => handleDelete(index)}
            >
              <X size={14} />
            </button>
          )}
        </span>
      ))}

      {!readOnly &&
        (isAdding ? (
          <span className="inline-flex items-center gap-1 px-2.5 py-1 rounded-md text-sm border border-blue-300 dark:border-blue-600 bg-blue-50 dark:bg-blue-900/30">
            <input
              ref={addInputRef}
              value={addValue}
              onChange={(e) => setAddValue(e.target.value)}
              onBlur={handleFinishAdd}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleFinishAdd();
                if (e.key === "Escape") {
                  setIsAdding(false);
                  setAddValue("");
                }
              }}
              placeholder={placeholder}
              className="bg-transparent outline-none border-none p-0 text-sm min-w-[80px]"
            />
          </span>
        ) : (
          <button
            type="button"
            className="cursor-pointer inline-flex items-center gap-1 px-2.5 py-1 rounded-md text-sm border border-dashed border-gray-300 dark:border-zinc-600 text-gray-500 dark:text-zinc-400 hover:border-blue-400 hover:text-blue-500 dark:hover:border-blue-500 dark:hover:text-blue-400 transition-colors"
            onClick={handleStartAdd}
          >
            <Plus size={14} />
          </button>
        ))}

      {value.length === 0 && readOnly && (
        <span className="text-sm text-gray-400 dark:text-zinc-500">—</span>
      )}
    </div>
  );
};

export default TagListEditor;
