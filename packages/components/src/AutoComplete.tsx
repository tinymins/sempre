import {
  FloatingPortal,
  flip,
  offset,
  size,
  useDismiss,
  useFloating,
  useInteractions,
} from "@floating-ui/react";
import { CircleX as ClearIcon } from "lucide-react";
import {
  forwardRef,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { cn } from "./utils";

export interface AutoCompleteOption {
  value: string;
  label?: ReactNode;
  disabled?: boolean;
}

export interface AutoCompleteProps {
  /** Options (strings or option objects) */
  options?: (string | AutoCompleteOption)[];
  /** Controlled value */
  value?: string;
  /** Default value */
  defaultValue?: string;
  /** On change (user types or selects) */
  onChange?: (value: string) => void;
  /** On select from dropdown */
  onSelect?: (value: string, option: AutoCompleteOption) => void;
  /** Custom filter (return true to keep) */
  filterOption?:
    | boolean
    | ((inputValue: string, option: AutoCompleteOption) => boolean);
  /** Placeholder */
  placeholder?: string;
  /** Disabled */
  disabled?: boolean;
  /** Allow clear */
  allowClear?: boolean;
  /** Size */
  size?: "small" | "middle" | "large";
  /** Status */
  status?: "error" | "warning";
  /** Auto focus */
  autoFocus?: boolean;
  /** On blur */
  onBlur?: () => void;
  /** On focus */
  onFocus?: () => void;
  /** Not found content */
  notFoundContent?: ReactNode;
  /** ClassName */
  className?: string;
  /** Style */
  style?: React.CSSProperties;
  /** Custom children input */
  children?: ReactNode;
  /** On search (typing) */
  onSearch?: (value: string) => void;
  /** Back fill selected value */
  backfill?: boolean;
  /** Custom dropdown render (antd compat) */
  dropdownRender?: (menu: ReactNode) => ReactNode;
  /** Keyboard event handler */
  onKeyDown?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
}

export const AutoComplete = forwardRef<HTMLDivElement, AutoCompleteProps>(
  function AutoComplete(
    {
      options: rawOptions = [],
      value: controlledVal,
      defaultValue = "",
      onChange,
      onSelect,
      filterOption = true,
      placeholder,
      disabled = false,
      allowClear = false,
      size: sz = "middle",
      status,
      autoFocus: _autoFocus,
      onBlur,
      onFocus,
      notFoundContent,
      className,
      style,
      children: _children,
      onSearch,
      backfill = false,
      onKeyDown: onKeyDownProp,
    },
    ref,
  ) {
    const [internal, setInternal] = useState(defaultValue);
    const val = controlledVal ?? internal;
    const [open, setOpen] = useState(false);
    const [activeIdx, setActiveIdx] = useState(-1);
    const inputRef = useRef<HTMLInputElement>(null);
    const interactedRef = useRef(false);

    const normalised: AutoCompleteOption[] = useMemo(
      () =>
        rawOptions.map((o) =>
          typeof o === "string" ? { value: o, label: o } : o,
        ),
      [rawOptions],
    );

    const filtered = useMemo(() => {
      if (filterOption === false) return normalised;
      const fn =
        typeof filterOption === "function"
          ? filterOption
          : (input: string, opt: AutoCompleteOption) =>
              opt.value.toLowerCase().includes(input.toLowerCase());
      return normalised.filter((o) => fn(val, o));
    }, [normalised, val, filterOption]);

    const { refs, floatingStyles, context } = useFloating({
      open,
      onOpenChange: setOpen,
      placement: "bottom-start",
      middleware: [
        offset(4),
        flip(),
        size({
          apply({ rects, elements }) {
            Object.assign(elements.floating.style, {
              minWidth: `${rects.reference.width}px`,
            });
          },
        }),
      ],
    });

    const dismiss = useDismiss(context);
    const { getReferenceProps, getFloatingProps } = useInteractions([dismiss]);

    const updateVal = useCallback(
      (v: string) => {
        setInternal(v);
        onChange?.(v);
      },
      [onChange],
    );

    const handleSelect = useCallback(
      (opt: AutoCompleteOption) => {
        updateVal(opt.value);
        onSelect?.(opt.value, opt);
        setOpen(false);
        inputRef.current?.focus();
      },
      [updateVal, onSelect],
    );

    const handleKeyDown = (e: React.KeyboardEvent) => {
      interactedRef.current = true;
      if (!open && e.key === "ArrowDown") {
        setOpen(true);
        return;
      }
      if (!open) return;

      if (e.key === "ArrowDown") {
        e.preventDefault();
        const next = activeIdx < filtered.length - 1 ? activeIdx + 1 : 0;
        setActiveIdx(next);
        if (backfill) updateVal(filtered[next].value);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        const prev = activeIdx > 0 ? activeIdx - 1 : filtered.length - 1;
        setActiveIdx(prev);
        if (backfill) updateVal(filtered[prev].value);
      } else if (e.key === "Enter") {
        if (activeIdx >= 0 && activeIdx < filtered.length) {
          e.preventDefault();
          handleSelect(filtered[activeIdx]);
        } else {
          setOpen(false);
        }
      } else if (e.key === "Escape") {
        setOpen(false);
      }
    };

    useEffect(() => {
      setActiveIdx(-1);
    }, []);

    const sizeClass = {
      small: "text-xs px-2",
      middle: "text-sm px-3",
      large: "text-base px-3",
    }[sz];

    const wrapperSizeClass = {
      small: "h-6",
      middle: "h-8",
      large: "h-10",
    }[sz];

    const borderClass = status
      ? status === "error"
        ? "border-red-500 focus-within:border-red-500 focus-within:ring-1 focus-within:ring-red-500/30"
        : "border-amber-500 focus-within:border-amber-500 focus-within:ring-1 focus-within:ring-amber-500/30"
      : "border-black/[0.08] dark:border-white/[0.1] focus-within:border-[var(--accent)] focus-within:ring-1 focus-within:ring-[var(--accent)]";

    return (
      <div
        ref={ref}
        className={cn("relative inline-block", className)}
        style={style}
      >
        <div
          ref={refs.setReference}
          {...getReferenceProps()}
          className={cn(
            "flex items-center rounded-md border transition-colors focus-within:ring-2",
            wrapperSizeClass,
            borderClass,
            disabled &&
              "opacity-50 cursor-not-allowed bg-black/[0.03] dark:bg-white/[0.02]",
          )}
        >
          <input
            ref={inputRef}
            type="text"
            className={cn(
              "flex-1 min-w-0 bg-transparent outline-none text-[var(--text-primary)] placeholder:text-[var(--text-muted)]",
              sizeClass,
              disabled && "cursor-not-allowed",
            )}
            value={val}
            placeholder={placeholder}
            disabled={disabled}
            onChange={(e) => {
              updateVal(e.target.value);
              onSearch?.(e.target.value);
              if (!open) setOpen(true);
            }}
            onMouseDown={() => {
              interactedRef.current = true;
            }}
            onFocus={() => {
              if (!disabled && interactedRef.current) setOpen(true);
              interactedRef.current = false;
              onFocus?.();
            }}
            onBlur={() => {
              interactedRef.current = false;
              onBlur?.();
            }}
            onKeyDown={(e) => {
              handleKeyDown(e);
              onKeyDownProp?.(e);
            }}
          />
          {allowClear && val ? (
            <button
              type="button"
              className="mr-2.5 cursor-pointer text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => {
                updateVal("");
                inputRef.current?.focus();
              }}
            >
              <ClearIcon className="w-3.5 h-3.5" />
            </button>
          ) : null}
        </div>

        {open && filtered.length > 0 && (
          <FloatingPortal>
            <div
              ref={refs.setFloating}
              style={floatingStyles}
              {...getFloatingProps()}
              className="z-[9999] bg-white/90 dark:bg-[rgba(15,15,25,0.9)] backdrop-blur-xl border border-black/[0.06] dark:border-white/[0.08] rounded-md shadow-lg py-1 max-h-64 overflow-auto"
            >
              {filtered.map((opt, i) => (
                <div
                  key={opt.value}
                  role="option"
                  tabIndex={0}
                  className={cn(
                    "px-3 py-1.5 text-sm cursor-pointer select-none transition-colors",
                    i === activeIdx
                      ? "bg-black/[0.04] dark:bg-white/[0.06] text-[var(--text-primary)]"
                      : "text-[var(--text-primary)] hover:bg-black/[0.04] dark:hover:bg-white/[0.06]",
                    opt.disabled && "opacity-40 cursor-not-allowed",
                  )}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => {
                    if (!opt.disabled) handleSelect(opt);
                  }}
                  onKeyDown={(e) => {
                    if ((e.key === "Enter" || e.key === " ") && !opt.disabled)
                      handleSelect(opt);
                  }}
                  onMouseEnter={() => setActiveIdx(i)}
                >
                  {opt.label ?? opt.value}
                </div>
              ))}
            </div>
          </FloatingPortal>
        )}
        {open && filtered.length === 0 && notFoundContent && (
          <FloatingPortal>
            <div
              ref={refs.setFloating}
              style={floatingStyles}
              {...getFloatingProps()}
              className="z-[9999] bg-white/90 dark:bg-[rgba(15,15,25,0.9)] backdrop-blur-xl border border-black/[0.06] dark:border-white/[0.08] rounded-md shadow-lg py-4 px-3 text-center text-sm text-[var(--text-muted)]"
            >
              {notFoundContent}
            </div>
          </FloatingPortal>
        )}
      </div>
    );
  },
);
