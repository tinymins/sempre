import { forwardRef, useCallback, useId, useMemo } from "react";
import { cn } from "./utils";

export interface SliderProps
  extends Omit<
    React.InputHTMLAttributes<HTMLInputElement>,
    "type" | "onChange" | "size"
  > {
  /** Current value */
  value?: number;
  /** Minimum value */
  min?: number;
  /** Maximum value */
  max?: number;
  /** Step increment */
  step?: number;
  /** Change handler (receives number, not event) */
  onChange?: (value: number) => void;
  /** Size variant */
  size?: "default" | "small";
  /** Custom accent color (CSS color string, defaults to var(--accent)) */
  accentColor?: string;
}

/**
 * iOS 26-style slider with filled track, white thumb, and solid colors.
 */
export const Slider = forwardRef<HTMLInputElement, SliderProps>(
  (
    {
      value: valueProp,
      defaultValue,
      min = 0,
      max = 100,
      step = 1,
      disabled = false,
      onChange,
      size = "default",
      accentColor,
      className,
      style,
      ...rest
    },
    ref,
  ) => {
    const id = useId();
    const value =
      valueProp ?? (typeof defaultValue === "number" ? defaultValue : 0);
    const pct = max > min ? ((value - min) / (max - min)) * 100 : 0;
    const fill = accentColor ?? "var(--accent)";

    const trackStyle = useMemo(
      () => ({
        ...style,
        background: `linear-gradient(to right, ${fill} ${pct}%, var(--slider-track, #d4d4d4) ${pct}%)`,
      }),
      [pct, fill, style],
    );

    const handleChange = useCallback(
      (e: React.ChangeEvent<HTMLInputElement>) => {
        onChange?.(Number(e.target.value));
      },
      [onChange],
    );

    const isSmall = size === "small";

    return (
      <input
        ref={ref}
        id={id}
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        onChange={handleChange}
        style={trackStyle}
        className={cn(
          "appearance-none rounded-full outline-none cursor-pointer",
          "[--slider-track:#d4d4d4] dark:[--slider-track:#525252]",
          isSmall ? "h-1" : "h-1.5",
          disabled && "opacity-50 cursor-not-allowed",
          // Webkit thumb
          "[&::-webkit-slider-thumb]:appearance-none",
          "[&::-webkit-slider-thumb]:rounded-full",
          "[&::-webkit-slider-thumb]:bg-white",
          "[&::-webkit-slider-thumb]:shadow-[0_1px_3px_rgba(0,0,0,0.25)]",
          "[&::-webkit-slider-thumb]:transition-shadow",
          "[&::-webkit-slider-thumb]:hover:shadow-[0_1px_5px_rgba(0,0,0,0.35)]",
          "[&::-webkit-slider-thumb]:active:shadow-[0_1px_6px_rgba(0,0,0,0.4)]",
          isSmall
            ? "[&::-webkit-slider-thumb]:w-3.5 [&::-webkit-slider-thumb]:h-3.5"
            : "[&::-webkit-slider-thumb]:w-[18px] [&::-webkit-slider-thumb]:h-[18px]",
          // Firefox thumb
          "[&::-moz-range-thumb]:appearance-none",
          "[&::-moz-range-thumb]:rounded-full",
          "[&::-moz-range-thumb]:border-0",
          "[&::-moz-range-thumb]:bg-white",
          "[&::-moz-range-thumb]:shadow-[0_1px_3px_rgba(0,0,0,0.25)]",
          "[&::-moz-range-thumb]:transition-shadow",
          "[&::-moz-range-thumb]:hover:shadow-[0_1px_5px_rgba(0,0,0,0.35)]",
          "[&::-moz-range-thumb]:active:shadow-[0_1px_6px_rgba(0,0,0,0.4)]",
          isSmall
            ? "[&::-moz-range-thumb]:w-3.5 [&::-moz-range-thumb]:h-3.5"
            : "[&::-moz-range-thumb]:w-[18px] [&::-moz-range-thumb]:h-[18px]",
          // Firefox track (hide default)
          "[&::-moz-range-track]:appearance-none",
          "[&::-moz-range-track]:bg-transparent",
          "[&::-moz-range-track]:border-0",
          className,
        )}
        {...rest}
      />
    );
  },
);
Slider.displayName = "Slider";
