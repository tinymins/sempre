import { ChevronDown, ChevronUp } from "lucide-react";
import { forwardRef, useState, type InputHTMLAttributes } from "react";
import { cn } from "./utils";

export interface InputNumberProps
  extends Omit<
    InputHTMLAttributes<HTMLInputElement>,
    "type" | "onChange" | "size" | "value"
  > {
  /** Current value */
  value?: number | null;
  /** Default value */
  defaultValue?: number;
  /** Change handler */
  onChange?: (value: number | null) => void;
  /** Min value */
  min?: number;
  /** Max value */
  max?: number;
  /** Step increment */
  step?: number;
  /** Size */
  size?: "small" | "middle" | "large";
  /** Disabled */
  disabled?: boolean;
  /** Precision (decimal places) */
  precision?: number;
  /** Addon before */
  addonBefore?: React.ReactNode;
  /** Addon after */
  addonAfter?: React.ReactNode;
  /** Controls visibility */
  controls?: boolean;
  /** Status */
  status?: "error" | "warning";
}

const sizeMap = {
  small: "h-6 text-xs",
  middle: "h-8 text-sm",
  large: "h-10 text-base",
};

export const InputNumber = forwardRef<HTMLInputElement, InputNumberProps>(
  (
    {
      value,
      defaultValue,
      onChange,
      min,
      max,
      step = 1,
      size = "middle",
      disabled = false,
      precision,
      addonBefore,
      addonAfter,
      controls = true,
      status,
      className,
      style,
      onBlur: onInputBlur,
      onKeyDown: onInputKeyDown,
      ...rest
    },
    ref,
  ) => {
    const [uncontrolledValue, setUncontrolledValue] = useState<number | null>(defaultValue ?? null);
    const [edit, setEdit] = useState<{ raw: string; external: number | null | undefined; emitted: number | null | undefined } | null>(null);
    const current = value === undefined ? uncontrolledValue : value;
    const editing = edit !== null && (value === undefined || value === edit.external || value === edit.emitted);
    const displayed = editing ? edit.raw : String(current ?? "");
    const parsed = (raw: string) => /^-?(?:\d+(?:\.\d*)?|\.\d+)$/.test(raw.trim()) ? Number(raw) : null;
    const clamp = (v: number) => {
      let val = v;
      if (min !== undefined) val = Math.max(min, val);
      if (max !== undefined) val = Math.min(max, val);
      if (precision !== undefined)
        val = Number.parseFloat(val.toFixed(precision));
      return val;
    };

    const emit = (next: number | null) => {
      if (value === undefined) setUncontrolledValue(next);
      onChange?.(next);
    };

    const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
      const raw = e.target.value;
      const num = parsed(raw);
      const inRange = num !== null && Number.isFinite(num) &&
        (min === undefined || num >= min) && (max === undefined || num <= max);
      const emitted = raw === "" ? null : inRange && num !== null ? clamp(num) : undefined;
      setEdit({ raw, external: value, emitted });
      if (emitted !== undefined) emit(emitted);
    };

    const commit = () => {
      if (!editing) { setEdit(null); return; }
      if (edit.raw.trim() === "") emit(null);
      else {
        const num = parsed(edit.raw);
        if (num !== null && Number.isFinite(num)) emit(clamp(num));
      }
      setEdit(null);
    };

    const increment = () => {
      const base = parsed(displayed) ?? current ?? 0;
      const next = clamp(base + step);
      setEdit({ raw: String(next), external: value, emitted: next });
      emit(next);
    };

    const decrement = () => {
      const base = parsed(displayed) ?? current ?? 0;
      const next = clamp(base - step);
      setEdit({ raw: String(next), external: value, emitted: next });
      emit(next);
    };

    return (
      <div
        className={cn(
          "inline-flex items-center rounded-md border bg-[var(--input-bg)] transition-colors focus-within:border-[var(--accent)] focus-within:ring-1 focus-within:ring-[var(--accent)] dark:focus-within:border-[var(--accent)]",
          status === "error"
            ? "border-red-500"
            : status === "warning"
              ? "border-amber-500"
              : "border-black/[0.08] dark:border-white/[0.1]",
          disabled && "opacity-50 cursor-not-allowed",
          sizeMap[size],
          className,
        )}
        style={style}
      >
        {addonBefore ? (
          <span className="px-2 text-[var(--text-muted)] border-r border-black/[0.08] dark:border-white/[0.1] bg-black/[0.03] dark:bg-white/[0.05] h-full flex items-center rounded-l-md text-sm">
            {addonBefore}
          </span>
        ) : null}
        <input
          {...rest}
          ref={ref}
          type="text"
          inputMode="decimal"
          disabled={disabled}
          value={displayed}
          onChange={handleChange}
          onBlur={(event) => { commit(); onInputBlur?.(event); }}
          onKeyDown={(event) => {
            if (event.key === "ArrowUp" || event.key === "ArrowDown") {
              event.preventDefault();
              if (event.key === "ArrowUp") increment(); else decrement();
            }
            onInputKeyDown?.(event);
          }}
          className="w-full min-w-[3em] bg-transparent outline-none text-left px-2"
        />
        {controls ? (
          <div className="flex flex-col border-l border-black/[0.08] dark:border-white/[0.1] shrink-0">
            <button
              type="button"
              tabIndex={-1}
              disabled={disabled || (max !== undefined && (current ?? 0) >= max)}
              onMouseDown={(event) => event.preventDefault()}
              className="px-1 hover:bg-black/[0.05] dark:hover:bg-white/[0.07] disabled:opacity-30 flex-1"
              onClick={increment}
            >
              <ChevronUp className="h-2.5 w-2.5" />
            </button>
            <button
              type="button"
              tabIndex={-1}
              disabled={disabled || (min !== undefined && (current ?? 0) <= min)}
              onMouseDown={(event) => event.preventDefault()}
              className="px-1 hover:bg-black/[0.05] dark:hover:bg-white/[0.07] disabled:opacity-30 flex-1 border-t border-black/[0.08] dark:border-white/[0.1]"
              onClick={decrement}
            >
              <ChevronDown className="h-2.5 w-2.5" />
            </button>
          </div>
        ) : null}
        {addonAfter ? (
          <span className="px-2 text-[var(--text-muted)] border-l border-black/[0.08] dark:border-white/[0.1] bg-black/[0.03] dark:bg-white/[0.05] h-full flex items-center rounded-r-md text-sm whitespace-nowrap">
            {addonAfter}
          </span>
        ) : null}
      </div>
    );
  },
);
InputNumber.displayName = "InputNumber";
