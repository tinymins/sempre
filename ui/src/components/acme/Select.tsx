import {
  autoUpdate,
  FloatingPortal,
  flip,
  offset,
  shift,
  size as sizeMiddleware,
  useClick,
  useDismiss,
  useFloating,
  useInteractions,
  useListNavigation,
  useRole,
} from "@floating-ui/react";
import { Check, ChevronDown, Search, X } from "lucide-react";
import React, {
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { FloatingVibrancy } from "./FloatingVibrancy";
import { cn } from "./utils";

export interface SelectOption {
  label: ReactNode;
  value: string | number;
  disabled?: boolean;
  /** Label shown in selected tags. Falls back to `label` if not set. */
  tagLabel?: ReactNode;
}

export interface SelectProps {
  /** Options */
  options?: SelectOption[];
  /** Value (controlled) */
  value?: string | number | (string | number)[];
  /** Default value */
  defaultValue?: string | number | (string | number)[];
  /** Change handler */
  // biome-ignore lint/suspicious/noExplicitAny: antd compat
  onChange?: (value: any, option?: any) => void;
  /** Placeholder */
  placeholder?: string;
  /** Allow clear */
  allowClear?: boolean;
  /** Multiple select */
  mode?: "multiple" | "tags";
  /** Disabled */
  disabled?: boolean;
  /** Size */
  size?: "small" | "middle" | "large";
  /** Status */
  status?: "error" | "warning";
  /** Loading */
  loading?: boolean;
  /** Show search */
  showSearch?: boolean;
  /** Filter option */
  filterOption?: boolean | ((input: string, option?: SelectOption) => boolean);
  /** Not found content */
  notFoundContent?: ReactNode;
  /** Style */
  style?: React.CSSProperties;
  className?: string;
  /** Dropdown class */
  popupClassName?: string;
  /** Option label prop (compatibility) */
  optionFilterProp?: string;
  /** Field names customization */
  fieldNames?: { label?: string; value?: string };
  /** Enable virtual scrolling for large option lists */
  virtual?: boolean;
  /** Children (Select.Option pattern) */
  children?: ReactNode;
}

const sizeMap = {
  small: "min-h-6 py-0 text-xs",
  middle: "min-h-8 py-0.5 text-sm",
  large: "min-h-10 py-1 text-base",
};

export function Select({
  options = [],
  value: valueProp,
  defaultValue,
  onChange,
  placeholder = "请选择",
  allowClear = false,
  mode,
  disabled = false,
  size = "middle",
  status,
  loading = false,
  showSearch = false,
  filterOption = true,
  notFoundContent,
  style,
  className,
  popupClassName,
  virtual = false,
  children,
}: SelectProps) {
  const isMultiple = mode === "multiple" || mode === "tags";
  const [internalValue, setInternalValue] = useState<
    string | number | (string | number)[] | undefined
  >(defaultValue);
  const value = valueProp ?? internalValue;

  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const listRef = useRef<Array<HTMLElement | null>>([]);
  const inputRef = useRef<HTMLInputElement>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);

  // Virtual scroll constants
  const ITEM_HEIGHT = 32;
  const MAX_HEIGHT = 240; // max-h-60
  const OVERSCAN = 5;

  // Scroll active item into view (for keyboard navigation)
  useEffect(() => {
    if (!virtual || activeIndex === null || !scrollContainerRef.current) return;
    const top = activeIndex * ITEM_HEIGHT;
    const container = scrollContainerRef.current;
    if (top < container.scrollTop) {
      container.scrollTop = top;
    } else if (top + ITEM_HEIGHT > container.scrollTop + MAX_HEIGHT) {
      container.scrollTop = top + ITEM_HEIGHT - MAX_HEIGHT;
    }
  }, [activeIndex, virtual]);

  const childOptions = useMemo(() => {
    if (options.length > 0) return options;
    const result: SelectOption[] = [];
    React.Children.forEach(children, (child) => {
      if (React.isValidElement(child) && child.props) {
        const props = child.props as {
          value?: string | number;
          disabled?: boolean;
          children?: ReactNode;
        };
        if (props.value !== undefined) {
          result.push({
            value: props.value,
            label: props.children ?? String(props.value),
            disabled: props.disabled,
          });
        }
      }
    });
    return result;
  }, [options, children]);

  const { refs, floatingStyles, context } = useFloating({
    open,
    onOpenChange: (v) => {
      if (disabled) return;
      setOpen(v);
      if (!v) setSearch("");
    },
    placement: "bottom-start",
    middleware: [
      offset(4),
      flip(),
      shift({ padding: 5 }),
      sizeMiddleware({
        apply({ rects, elements }) {
          Object.assign(elements.floating.style, {
            minWidth: `${rects.reference.width}px`,
          });
        },
      }),
    ],
    whileElementsMounted: autoUpdate,
  });

  const click = useClick(context, { enabled: !disabled });
  const dismiss = useDismiss(context);
  const role = useRole(context, { role: "listbox" });
  const listNav = useListNavigation(context, {
    listRef,
    activeIndex,
    onNavigate: setActiveIndex,
    virtual: true,
    loop: true,
  });

  const { getReferenceProps, getFloatingProps, getItemProps } = useInteractions(
    [click, dismiss, role, listNav],
  );

  // Filter options
  const filtered =
    search && filterOption
      ? childOptions.filter((opt) => {
          if (typeof filterOption === "function") {
            return filterOption(search, opt);
          }
          const label =
            typeof opt.label === "string" ? opt.label : String(opt.value);
          return label.toLowerCase().includes(search.toLowerCase());
        })
      : childOptions;

  // Selection helpers
  const selectedValues = isMultiple
    ? Array.isArray(value)
      ? value
      : value !== undefined
        ? [value]
        : []
    : [];

  const singleValue = !isMultiple ? value : undefined;

  const isSelected = (optValue: string | number) =>
    isMultiple ? selectedValues.includes(optValue) : singleValue === optValue;

  const findNextEnabledIndex = (
    startIndex: number,
    direction: 1 | -1,
  ): number | null => {
    if (filtered.length === 0) {
      return null;
    }

    for (let step = 1; step <= filtered.length; step += 1) {
      const index =
        (startIndex + direction * step + filtered.length) % filtered.length;
      const option = filtered[index];
      if (option && !option.disabled) {
        return index;
      }
    }

    return null;
  };

  const handleSelect = (optValue: string | number) => {
    if (isMultiple) {
      const current = [...selectedValues];
      const idx = current.indexOf(optValue);
      if (idx >= 0) {
        current.splice(idx, 1);
      } else {
        current.push(optValue);
      }
      if (valueProp === undefined) setInternalValue(current);
      onChange?.(current);
      setSearch("");
      setActiveIndex(null);
    } else {
      if (valueProp === undefined) setInternalValue(optValue);
      onChange?.(optValue);
      setOpen(false);
      setSearch("");
      setActiveIndex(null);
    }
  };

  const handleSearchKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Backspace" && isMultiple && search.length === 0) {
      const lastValue = selectedValues[selectedValues.length - 1];
      if (lastValue !== undefined) {
        event.preventDefault();
        event.stopPropagation();
        handleSelect(lastValue);
      }
      return;
    }

    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      event.stopPropagation();
      if (!open) {
        setOpen(true);
      }
      const direction = event.key === "ArrowDown" ? 1 : -1;
      const nextIndex =
        activeIndex === null
          ? findNextEnabledIndex(direction === 1 ? -1 : 0, direction)
          : findNextEnabledIndex(activeIndex, direction);
      setActiveIndex(nextIndex);
      return;
    }

    if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      const candidate =
        (activeIndex !== null ? filtered[activeIndex] : undefined) ??
        (filtered.length === 1 ? filtered[0] : undefined);
      if (candidate && !candidate.disabled) {
        handleSelect(candidate.value);
      }
    }
  };

  const handleClear = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (isMultiple) {
      if (valueProp === undefined) setInternalValue([]);
      onChange?.([] as string[]);
    } else {
      if (valueProp === undefined) setInternalValue(undefined);
      onChange?.(undefined as unknown as string);
    }
  };

  // Display
  const getLabel = (v: string | number) => {
    const opt = childOptions.find((o) => o.value === v);
    return opt?.label ?? v;
  };
  const getTagLabel = (v: string | number) => {
    const opt = childOptions.find((o) => o.value === v);
    return opt?.tagLabel ?? opt?.label ?? v;
  };

  const hasValue = isMultiple
    ? selectedValues.length > 0
    : value !== undefined &&
      value !== null &&
      (value !== "" || childOptions.some((opt) => opt.value === ""));

  useEffect(() => {
    if (open && showSearch) {
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open, showSearch]);

  useEffect(() => {
    setActiveIndex((current) => {
      if (current === null) {
        return null;
      }
      return filtered[current] ? current : null;
    });
    if (virtual && scrollContainerRef.current) {
      scrollContainerRef.current.scrollTop = 0;
    }
  }, [filtered, virtual]);

  return (
    <>
      <div
        ref={refs.setReference}
        className={cn(
          "inline-flex items-center gap-1 px-2 rounded-md border bg-[var(--input-bg)] cursor-pointer transition-colors",
          open
            ? "border-[var(--accent)] ring-1 ring-[var(--accent)]"
            : status === "error"
              ? "border-red-500"
              : status === "warning"
                ? "border-amber-500"
                : "border-black/[0.08] dark:border-white/[0.1]",
          disabled &&
            "opacity-50 cursor-not-allowed bg-black/[0.02] dark:bg-white/[0.02]",
          sizeMap[size],
          className,
        )}
        style={{
          ...style,
        }}
        {...getReferenceProps()}
      >
        <div className="flex-1 flex items-center gap-1 overflow-hidden min-w-0">
          {isMultiple ? (
            <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1">
              {selectedValues.map((v) => (
                <span
                  key={String(v)}
                  className="inline-flex items-center gap-0.5 bg-black/[0.04] dark:bg-white/[0.08] text-xs rounded px-1.5 py-0.5"
                >
                  {getTagLabel(v)}
                  {!disabled && (
                    <button
                      type="button"
                      className="cursor-pointer hover:text-red-500"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleSelect(v);
                      }}
                    >
                      <X className="h-3 w-3" />
                    </button>
                  )}
                </span>
              ))}
              {showSearch ? (
                <input
                  ref={inputRef}
                  className="min-w-16 flex-1 bg-transparent py-0.5 text-sm outline-none placeholder:text-[var(--text-muted)]"
                  placeholder={
                    selectedValues.length === 0 ? placeholder : undefined
                  }
                  value={search}
                  onChange={(e) => {
                    setSearch(e.target.value);
                    if (!open) setOpen(true);
                  }}
                  onKeyDown={handleSearchKeyDown}
                  onClick={(e) => e.stopPropagation()}
                  onFocus={() => {
                    if (!open) setOpen(true);
                  }}
                  disabled={disabled}
                />
              ) : selectedValues.length === 0 ? (
                <span className="text-[var(--text-muted)] truncate">
                  {placeholder}
                </span>
              ) : null}
            </div>
          ) : hasValue ? (
            <span className="truncate text-[var(--text-primary)]">
              {getLabel(value as string | number)}
            </span>
          ) : (
            <span className="text-[var(--text-muted)] truncate">
              {placeholder}
            </span>
          )}
        </div>
        {allowClear && hasValue && !disabled ? (
          <button
            type="button"
            className="shrink-0 cursor-pointer text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
            onClick={handleClear}
          >
            <X className="h-3.5 w-3.5" />
          </button>
        ) : null}
        {loading ? (
          <svg
            className="h-3.5 w-3.5 animate-spin text-[var(--text-muted)] shrink-0"
            viewBox="0 0 24 24"
            fill="none"
          >
            <circle
              className="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              strokeWidth="4"
            />
            <path
              className="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
            />
          </svg>
        ) : (
          <ChevronDown
            className={cn(
              "h-3.5 w-3.5 shrink-0 text-[var(--text-muted)] transition-transform",
              open && "rotate-180",
            )}
          />
        )}
      </div>

      {open ? (
        <FloatingPortal>
          <div
            ref={refs.setFloating}
            style={{
              ...floatingStyles,
              backdropFilter: "blur(var(--window-blur, 24px))",
              WebkitBackdropFilter: "blur(var(--window-blur, 24px))",
              borderRadius: "var(--window-radius, 8px)",
            }}
            className={cn(
              "z-[9999] bg-[rgba(255,255,255,calc(var(--window-opacity,85)/100))] dark:bg-[rgba(15,15,25,calc(var(--window-opacity,85)/100))] border border-black/[0.06] dark:border-white/[0.08] shadow-lg overflow-hidden",
              popupClassName,
            )}
            {...getFloatingProps()}
          >
            <FloatingVibrancy />
            {showSearch && !isMultiple ? (
              <div className="relative p-2 border-b border-black/[0.06] dark:border-white/[0.08]">
                <div className="flex items-center gap-1.5 px-2 py-1 bg-black/[0.03] dark:bg-white/[0.04] rounded">
                  <Search className="h-3.5 w-3.5 text-[var(--text-muted)]" />
                  <input
                    ref={inputRef}
                    className="flex-1 bg-transparent outline-none text-sm placeholder:text-[var(--text-muted)]"
                    placeholder="搜索..."
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                    onKeyDown={handleSearchKeyDown}
                  />
                </div>
              </div>
            ) : null}
            <div
              ref={scrollContainerRef}
              className="relative max-h-60 overflow-y-auto py-1"
              style={{ scrollbarWidth: "thin" }}
            >
              {filtered.length === 0 ? (
                <div className="px-3 py-4 text-center text-sm text-[var(--text-muted)]">
                  {notFoundContent ?? "无匹配选项"}
                </div>
              ) : virtual ? (
                <VirtualList
                  items={filtered}
                  scrollContainerRef={scrollContainerRef}
                  itemHeight={ITEM_HEIGHT}
                  maxHeight={MAX_HEIGHT}
                  overscan={OVERSCAN}
                  listRef={listRef}
                  activeIndex={activeIndex}
                  isSelected={isSelected}
                  getItemProps={getItemProps}
                  handleSelect={handleSelect}
                />
              ) : (
                filtered.map((opt, i) => {
                  const selected = isSelected(opt.value);

                  return (
                    <div
                      key={String(opt.value)}
                      ref={(node) => {
                        listRef.current[i] = node;
                      }}
                      className={cn(
                        "flex items-center gap-2 px-3 py-1.5 text-sm cursor-pointer transition-colors",
                        selected
                          ? "text-[var(--accent)] bg-[var(--accent-subtle)]"
                          : "text-[var(--text-primary)]",
                        !selected &&
                          activeIndex === i &&
                          "bg-black/[0.04] dark:bg-white/[0.06]",
                        opt.disabled && "opacity-50 cursor-not-allowed",
                      )}
                      {...getItemProps({
                        onClick: () => {
                          if (!opt.disabled) handleSelect(opt.value);
                        },
                      })}
                    >
                      <span className="flex-1">{opt.label}</span>
                      {selected ? <Check className="h-4 w-4 shrink-0" /> : null}
                    </div>
                  );
                })
              )}
            </div>
          </div>
        </FloatingPortal>
      ) : null}
    </>
  );
}

function VirtualList({
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

function SelectOption(_props: {
  value: string | number;
  disabled?: boolean;
  children?: ReactNode;
}) {
  return null; // Rendered by parent Select
}
Select.Option = SelectOption;
