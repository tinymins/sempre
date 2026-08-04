import type { ReactNode } from "react";
import { cn } from "./utils";

/* ─── Statistic ─── */
export interface StatisticProps {
  /** Title label */
  title?: ReactNode;
  /** Value */
  value?: string | number;
  /** Prefix element */
  prefix?: ReactNode;
  /** Suffix element */
  suffix?: ReactNode;
  /** Value style */
  valueStyle?: React.CSSProperties;
  /** Precision (decimal places) */
  precision?: number;
  /** Loading state */
  loading?: boolean;
  className?: string;
}

export function Statistic({
  title,
  value,
  prefix,
  suffix,
  valueStyle,
  precision,
  loading = false,
  className,
}: StatisticProps) {
  let displayValue: string | number | undefined = value;
  if (typeof value === "number" && precision !== undefined) {
    displayValue = value.toFixed(precision);
  }

  return (
    <div className={cn("text-left", className)}>
      {title ? <div className="text-sm text-fg-muted mb-2">{title}</div> : null}
      {loading ? (
        <div className="h-8 w-20 bg-fill-tertiary rounded animate-pulse" />
      ) : (
        <div
          className="text-2xl font-semibold text-fg-primary flex items-baseline gap-1"
          style={valueStyle}
        >
          {prefix ? (
            <span className="inline-flex translate-y-[0.125em] [&>svg]:w-[1em] [&>svg]:h-[1em]">
              {prefix}
            </span>
          ) : null}
          <span>{displayValue}</span>
          {suffix ? (
            <span className="text-base font-normal text-fg-muted">
              {suffix}
            </span>
          ) : null}
        </div>
      )}
    </div>
  );
}
