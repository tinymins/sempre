import { cn } from "./utils";

export interface SegmentedToggleProps {
  /** Controlled value */
  value?: boolean;
  /** Default value (uncontrolled) */
  defaultValue?: boolean;
  /** Change handler */
  onChange?: (value: boolean) => void;
  /** Label shown when value is true */
  checkedLabel?: React.ReactNode;
  /** Label shown when value is false */
  uncheckedLabel?: React.ReactNode;
  disabled?: boolean;
  className?: string;
}

export function SegmentedToggle({
  value,
  defaultValue = false,
  onChange,
  checkedLabel = "On",
  uncheckedLabel = "Off",
  disabled = false,
  className,
}: SegmentedToggleProps) {
  const isOn = value ?? defaultValue;

  return (
    <div
      className={cn(
        "relative flex w-full rounded-md p-0.5 bg-[var(--accent-subtle)]",
        disabled && "cursor-not-allowed opacity-50",
        className,
      )}
    >
      {/* sliding pill */}
      <span
        className={cn(
          "absolute inset-y-0.5 w-[calc(50%-2px)] rounded-[5px] bg-[var(--accent)] shadow-sm transition-[left] duration-200 ease-in-out",
          isOn ? "left-0.5" : "left-[calc(50%+1px)]",
        )}
      />
      <button
        type="button"
        disabled={disabled}
        className={cn(
          "relative z-10 flex-1 py-1.5 text-sm font-medium transition-colors duration-200 select-none",
          isOn ? "text-white" : "text-white/40",
        )}
        onClick={() => !disabled && onChange?.(true)}
      >
        {checkedLabel}
      </button>
      <button
        type="button"
        disabled={disabled}
        className={cn(
          "relative z-10 flex-1 py-1.5 text-sm font-medium transition-colors duration-200 select-none",
          !isOn ? "text-white" : "text-white/40",
        )}
        onClick={() => !disabled && onChange?.(false)}
      >
        {uncheckedLabel}
      </button>
    </div>
  );
}
