import type { LucideProps } from "lucide-react";
import { type CSSProperties, useRef, useState } from "react";
import { HolderOutlined } from "./icons";
import { cn } from "./utils";

/* ─── DragHandle ─── */

export interface DragHandleProps extends LucideProps {
  /** Render as disabled (not-allowed cursor, dimmed) */
  disabled?: boolean;
  /** Whether a drag is currently in progress (shows grabbing cursor) */
  isDragging?: boolean;
}

/** Grip icon used as a drag handle. Spread `getHandleProps(i)` onto it. */
export function DragHandle({
  disabled,
  isDragging,
  className,
  ...props
}: DragHandleProps) {
  return (
    <HolderOutlined
      className={cn(
        "text-fg-muted touch-none",
        disabled
          ? "cursor-not-allowed opacity-30"
          : isDragging
            ? "cursor-grabbing"
            : "cursor-grab hover:text-fg-secondary",
        className,
      )}
      {...props}
    />
  );
}

/* ─── useDnd ─── */

export interface UseDndOptions {
  /** Number of sortable items */
  count: number;
  /** Called when a drag completes at a different position */
  onReorder?: (fromIndex: number, toIndex: number) => void;
  /** Disable all dragging */
  disabled?: boolean;
}

export interface UseDndReturn {
  /** Inline style for an item at `index` — provides CSS transform animation */
  getItemStyle: (index: number) => CSSProperties;
  /** Props to spread on each sortable item element (`data-dnd-index`) */
  getItemProps: (index: number) => { "data-dnd-index": number };
  /** Props to spread on (or pass to) a drag handle for the item at `index` */
  getHandleProps: (index: number) => {
    onPointerDown: (e: React.PointerEvent) => void;
  };
  /** Whether a drag is currently in progress */
  isDragging: boolean;
  /** Index of the item being dragged, or -1 */
  activeIndex: number;
  /** True from drop until disabled is cleared by consumer */
  isPending: boolean;
}

/**
 * Generic pointer-event-based vertical drag-to-reorder hook.
 *
 * Each sortable item must have `data-dnd-index` (via `getItemProps`).
 * A drag handle inside the item receives `getHandleProps`.
 * Apply `getItemStyle` to each item for smooth CSS-transform animation.
 */
export function useDnd({
  count,
  onReorder,
  disabled = false,
}: UseDndOptions): UseDndReturn {
  const dragFromRef = useRef(-1);
  const dragOverRef = useRef(-1);
  const itemHeightRef = useRef(0);
  const containerTopRef = useRef(0);
  /** Internal lock — stays true from drop until the consumer re-enables */
  const pendingRef = useRef(false);

  // Keep disabled in a ref so closures always see the latest value
  const disabledRef = useRef(disabled);
  disabledRef.current = disabled;

  // When the consumer clears disabled (mutation settled), also clear lock
  if (!disabled) pendingRef.current = false;

  const [dragRender, setDragRender] = useState<{
    from: number;
    over: number;
  } | null>(null);

  const handlePointerDown = (idx: number, e: React.PointerEvent) => {
    if (disabledRef.current || pendingRef.current) return;
    e.preventDefault();

    const item = (e.target as HTMLElement).closest(
      "[data-dnd-index]",
    ) as HTMLElement | null;
    if (!item) return;

    dragFromRef.current = idx;
    dragOverRef.current = idx;
    itemHeightRef.current = item.getBoundingClientRect().height;
    const container = item.parentElement;
    if (container)
      containerTopRef.current = container.getBoundingClientRect().top;

    document.body.style.cursor = "grabbing";
    document.body.style.userSelect = "none";
    setDragRender({ from: idx, over: idx });

    const total = count;

    const onMove = (ev: PointerEvent) => {
      const h = itemHeightRef.current;
      if (!h || dragFromRef.current < 0) return;
      const raw = Math.floor((ev.clientY - containerTopRef.current) / h);
      const clamped = Math.max(0, Math.min(raw, total - 1));
      if (clamped !== dragOverRef.current) {
        dragOverRef.current = clamped;
        setDragRender({ from: dragFromRef.current, over: clamped });
      }
    };

    const onUp = () => {
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";

      const from = dragFromRef.current;
      const to = dragOverRef.current;
      dragFromRef.current = -1;
      dragOverRef.current = -1;
      setDragRender(null);

      if (from !== to && from >= 0 && !disabledRef.current) {
        pendingRef.current = true;
        onReorder?.(from, to);
      }
    };

    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
  };

  const getItemStyle = (idx: number): CSSProperties => {
    if (!dragRender) return {};
    const { from, over } = dragRender;
    const h = itemHeightRef.current;

    const transition = "transform 200ms cubic-bezier(.2,0,0,1)";

    if (from === over || !h) {
      return { transform: "translateY(0)", transition };
    }

    if (idx === from) {
      return {
        transform: `translateY(${(over - from) * h}px)`,
        transition,
        opacity: 0.85,
        zIndex: 1,
        position: "relative",
        boxShadow: "0 2px 12px rgba(0,0,0,0.12)",
      };
    }
    if (from < over && idx > from && idx <= over) {
      return { transform: `translateY(-${h}px)`, transition };
    }
    if (from > over && idx >= over && idx < from) {
      return { transform: `translateY(${h}px)`, transition };
    }
    return { transform: "translateY(0)", transition };
  };

  return {
    getItemStyle,
    getItemProps: (index: number) => ({ "data-dnd-index": index }),
    getHandleProps: (index: number) => ({
      onPointerDown: (e: React.PointerEvent) => handlePointerDown(index, e),
    }),
    isDragging: dragRender !== null,
    activeIndex: dragFromRef.current,
    /** True from drop until disabled is cleared by consumer */
    isPending: pendingRef.current,
  };
}
