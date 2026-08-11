import { Check } from "lucide-react";
import React, { useEffect, useState } from "react";
import type { SelectOption } from "./Select.types";
import { cn } from "./utils";

export function VirtualList({
  items,
  scrollContainerRef,
  itemHeight,
  maxHeight,
  overscan,
  listRef,
  activeIndex,
  isSelected,
  getItemProps,
  handleSelect,
}: {
  items: SelectOption[];
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  itemHeight: number;
  maxHeight: number;
  overscan: number;
  listRef: React.MutableRefObject<Array<HTMLElement | null>>;
  activeIndex: number | null;
  isSelected: (v: string | number) => boolean;
  // biome-ignore lint/suspicious/noExplicitAny: floating-ui compat
  getItemProps: (props?: any) => Record<string, unknown>;
  handleSelect: (v: string | number) => void;
}) {
  const [scrollTop, setScrollTop] = useState(0);

  useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container) return;
    // Read initial DOM scrollTop (0 for fresh mount)
    setScrollTop(container.scrollTop);
    const onScroll = () => setScrollTop(container.scrollTop);
    container.addEventListener("scroll", onScroll, { passive: true });
    return () => container.removeEventListener("scroll", onScroll);
  }, [scrollContainerRef]);

  const totalHeight = items.length * itemHeight;
  const visibleCount = Math.ceil(maxHeight / itemHeight);
  const startIndex = Math.max(0, Math.floor(scrollTop / itemHeight) - overscan);
  const endIndex = Math.min(
    items.length,
    startIndex + visibleCount + 2 * overscan,
  );

  return (
    <div style={{ height: totalHeight, position: "relative" }}>
      {items.slice(startIndex, endIndex).map((opt, localIdx) => {
        const i = startIndex + localIdx;
        const selected = isSelected(opt.value);
        return (
          <div
            key={String(opt.value)}
            ref={(node) => {
              listRef.current[i] = node;
            }}
            className={cn(
              "flex items-center gap-2 px-3 text-sm cursor-pointer transition-colors absolute inset-x-0",
              selected
                ? "text-[var(--accent)] bg-[var(--accent-subtle)]"
                : "text-[var(--text-primary)]",
              !selected &&
                activeIndex === i &&
                "bg-black/[0.04] dark:bg-white/[0.06]",
              opt.disabled && "opacity-50 cursor-not-allowed",
            )}
            style={{ height: itemHeight, top: i * itemHeight }}
            {...getItemProps({
              onClick: () => {
                if (!opt.disabled) handleSelect(opt.value);
              },
            })}
          >
            <span className="flex-1 truncate">{opt.label}</span>
            {selected ? <Check className="h-4 w-4 shrink-0" /> : null}
          </div>
        );
      })}
    </div>
  );
}

