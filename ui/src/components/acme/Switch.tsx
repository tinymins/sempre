import { forwardRef } from "react";
import { cn } from "./utils";

export interface SwitchProps {
  /** Whether checked */
  checked?: boolean;
  /** Default checked state */
  defaultChecked?: boolean;
  /** Change handler */
  onChange?: (checked: boolean) => void;
  /** Disabled state */
  disabled?: boolean;
  /** Size */
  size?: "default" | "small";
  /** Checked label */
  checkedChildren?: React.ReactNode;
  /** Unchecked label */
  unCheckedChildren?: React.ReactNode;
  /** Loading state */
  loading?: boolean;
  className?: string;
  /** Value alias for form usage (antd compatibility) */
  value?: boolean;
}

export const Switch = forwardRef<HTMLButtonElement, SwitchProps>(
  (
    {
      checked: checkedProp,
      defaultChecked = false,
      onChange,
      disabled = false,
      size = "default",
      checkedChildren,
      unCheckedChildren,
      loading = false,
      className,
      value,
    },
    ref,
  ) => {
    // Support both `checked` and `value` (antd Form passes `value`)
    const isChecked = checkedProp ?? value ?? defaultChecked;

    return (
      <button
        ref={ref}
        type="button"
        role="switch"
        aria-checked={isChecked}
        disabled={disabled || loading}
        className={cn(
          "relative inline-flex shrink-0 cursor-pointer items-center rounded-full transition-colors duration-200",
          isChecked ? "bg-[var(--accent)]" : "bg-fill-tertiary",
          disabled && "opacity-50 cursor-not-allowed",
          size === "small" ? "h-4 w-7" : "h-6 w-11",
          className,
        )}
        onClick={() => {
          if (!disabled && !loading) onChange?.(!isChecked);
        }}
      >
        {/* Labels */}
        {size !== "small" && (checkedChildren || unCheckedChildren) ? (
          <span
            className={cn(
              "absolute text-white text-xs px-1",
              isChecked ? "left-1" : "right-1",
            )}
          >
            {isChecked ? checkedChildren : unCheckedChildren}
          </span>
        ) : null}
        {/* Thumb */}
        <span
          className={cn(
            "inline-block rounded-full bg-white shadow transition-transform duration-200",
            size === "small"
              ? "h-3 w-3 translate-x-0.5"
              : "h-5 w-5 translate-x-0.5",
            isChecked &&
              (size === "small" ? "translate-x-[14px]" : "translate-x-[22px]"),
          )}
        >
          {loading ? (
            <svg
              className="h-full w-full animate-spin text-sky-500 p-0.5"
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
          ) : null}
        </span>
      </button>
    );
  },
);
Switch.displayName = "Switch";
