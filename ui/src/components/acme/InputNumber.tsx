import { ChevronDown, ChevronUp } from "lucide-react";
import { forwardRef, type InputHTMLAttributes } from "react";
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
      ...rest
    },
    ref,
  ) => {
    const clamp = (v: number) => {
      let val = v;
      if (min !== undefined) val = Math.max(min, val);
      if (max !== undefined) val = Math.min(max, val);
      if (precision !== undefined)
        val = Number.parseFloat(val.toFixed(precision));
      return val;
    };

    const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
      const raw = e.target.value;
      if (raw === "" || raw === "-") {
        onChange?.(null);
        return;
      }
      const num = Number.parseFloat(raw);
      if (!Number.isNaN(num)) {
        onChange?.(clamp(num));
      }
    };

    const increment = () => {
      const current = value ?? defaultValue ?? 0;
      onChange?.(clamp(current + step));
    };

    const decrement = () => {
      const current = value ?? defaultValue ?? 0;
      onChange?.(clamp(current - step));
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
          ref={ref}
          type="text"
          inputMode="decimal"
          disabled={disabled}
          value={value ?? ""}
          onChange={handleChange}
          className="w-full min-w-[3em] bg-transparent outline-none text-left px-2"
          {...rest}
        />
        {controls ? (
          <div className="flex flex-col border-l border-black/[0.08] dark:border-white/[0.1] shrink-0">
            <button
              type="button"
              tabIndex={-1}
              disabled={disabled || (max !== undefined && (value ?? 0) >= max)}
              className="px-1 hover:bg-black/[0.05] dark:hover:bg-white/[0.07] disabled:opacity-30 flex-1"
              onClick={increment}
            >
              <ChevronUp className="h-2.5 w-2.5" />
            </button>
            <button
              type="button"
              tabIndex={-1}
              disabled={disabled || (min !== undefined && (value ?? 0) <= min)}
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
