import { type ReactNode, useEffect, useRef, useState } from "react";
import { cn } from "./utils";

interface PathBarSegment {
  name: string;
  path: string;
}

export interface PathBarProps {
  /** Current full path, e.g. "/media/movies" */
  path: string;
  /** Called when a segment or typed path is navigated to */
  onNavigate: (path: string) => void;
  /** Custom root element (default: "/" text) */
  rootLabel?: ReactNode;
  /** Custom separator between segments (default: "›") */
  separator?: ReactNode;
  /** Custom renderer for each segment */
  renderSegment?: (segment: PathBarSegment, isLast: boolean) => ReactNode;
  /** Extra content after the breadcrumb (e.g. close button) */
  suffix?: ReactNode;
  className?: string;
}

function parsePath(p: string): PathBarSegment[] {
  if (p === "/" || !p) return [];
  const parts = p.replace(/\/$/, "").split("/").filter(Boolean);
  return parts.map((name, idx) => ({
    name,
    path: `/${parts.slice(0, idx + 1).join("/")}`,
  }));
}

export function PathBar({
  path,
  onNavigate,
  rootLabel,
  separator,
  renderSegment,
  suffix,
  className,
}: PathBarProps) {
  const [editing, setEditing] = useState(false);
  const [editValue, setEditValue] = useState(path);
  const inputRef = useRef<HTMLInputElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const segments = parsePath(path);

  // Auto-scroll to end after every render in display mode
  const prevEditingRef = useRef(editing);
  useEffect(() => {
    const el = scrollRef.current;
    if (el && !editing) el.scrollLeft = el.scrollWidth;
    prevEditingRef.current = editing;
  });

  // Focus input when entering edit mode (cursor at end, no select)
  useEffect(() => {
    if (editing) {
      const input = inputRef.current;
      if (input) {
        input.focus();
        // Place cursor at end
        const len = input.value.length;
        input.setSelectionRange(len, len);
      }
    }
  }, [editing]);

  const startEdit = () => {
    setEditValue(path);
    setEditing(true);
  };

  const submitEdit = () => {
    const trimmed = editValue.trim();
    if (trimmed.startsWith("/") && trimmed !== path) {
      onNavigate(trimmed);
    }
    setEditing(false);
  };

  const cancelEdit = () => {
    setEditing(false);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      submitEdit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelEdit();
    }
  };

  const defaultSep = (
    <span className="shrink-0 opacity-30 text-[10px] mx-0.5 select-none">
      ›
    </span>
  );

  if (editing) {
    return (
      <div
        ref={scrollRef}
        className={cn(
          "flex items-center gap-0.5 min-w-0 overflow-x-auto text-sm select-none",
          className,
        )}
        style={{ scrollbarWidth: "none" }}
      >
        {/* Root segment — always visible */}
        <button
          type="button"
          className="flex items-center gap-1 px-1.5 py-0.5 rounded transition-colors cursor-pointer hover:bg-black/[0.06] dark:hover:bg-white/[0.08] shrink-0"
          style={{ color: "var(--text-tertiary)" }}
          onClick={() => {
            onNavigate("/");
            setEditing(false);
          }}
        >
          {rootLabel ?? <span>/</span>}
        </button>

        {/* Input replaces segments */}
        <input
          ref={inputRef}
          type="text"
          value={editValue}
          onChange={(e) => setEditValue(e.target.value)}
          onKeyDown={handleKeyDown}
          onBlur={submitEdit}
          className="flex-1 min-w-0 min-h-[24px] px-1 py-0.5 bg-transparent outline-none text-inherit font-mono text-sm select-text"
        />

        {/* Suffix — always visible */}
        {suffix && (
          <div className="flex items-center ml-auto shrink-0">{suffix}</div>
        )}
      </div>
    );
  }

  return (
    <div
      ref={scrollRef}
      className={cn(
        "flex items-center gap-0.5 min-w-0 overflow-x-auto text-sm select-none",
        className,
      )}
      style={{ scrollbarWidth: "none" }}
    >
      {/* Root segment — always visible, not part of edit trigger */}
      <button
        type="button"
        className="flex items-center gap-1 px-1.5 py-0.5 rounded transition-colors cursor-pointer hover:bg-black/[0.06] dark:hover:bg-white/[0.08] shrink-0"
        style={{
          color: segments.length === 0 ? undefined : "var(--text-tertiary)",
        }}
        onClick={() => segments.length > 0 && onNavigate("/")}
      >
        {rootLabel ?? <span>/</span>}
      </button>

      {/* Segments area — click empty space to start editing */}
      <div
        className="flex flex-1 items-center gap-0.5 min-w-0 min-h-[24px] cursor-text"
        onClick={startEdit}
      >
        <div
          className="flex items-center gap-0.5 shrink-0"
          onClick={(e) => e.stopPropagation()}
        >
          {segments.map((seg, idx) => {
            const isLast = idx === segments.length - 1;
            return (
              <span
                key={seg.path}
                className="flex items-center gap-0.5 shrink-0"
              >
                {separator ?? defaultSep}
                {renderSegment ? (
                  renderSegment(seg, isLast)
                ) : (
                  <button
                    type="button"
                    onClick={() => !isLast && onNavigate(seg.path)}
                    className="px-1.5 py-0.5 rounded transition-colors hover:bg-black/[0.06] dark:hover:bg-white/[0.08]"
                    style={{
                      color: isLast ? undefined : "var(--text-tertiary)",
                      fontWeight: isLast ? 500 : 400,
                      cursor: isLast ? "default" : "pointer",
                    }}
                  >
                    {seg.name}
                  </button>
                )}
              </span>
            );
          })}
        </div>
      </div>

      {/* Spacer + suffix (e.g. close button) */}
      {suffix && (
        <div className="flex items-center ml-auto shrink-0">{suffix}</div>
      )}
    </div>
  );
}
