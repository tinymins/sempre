import type { ReactNode } from "react";
import { cn } from "./utils";

export interface ProgressProps {
  /** Percentage value (0-100) */
  percent?: number;
  /** Progress type */
  type?: "line" | "circle";
  /** Status */
  status?: "normal" | "success" | "exception" | "active";
  /** Show percentage text */
  showInfo?: boolean;
  /** Line width for circle, or bar height for line */
  strokeWidth?: number;
  /** Circle diameter */
  width?: number;
  /** Trail color */
  trailColor?: string;
  /** Stroke color */
  strokeColor?: string;
  /** Size variant or circle diameter number */
  size?: "default" | "small" | number;
  /** Custom format function */
  format?: (percent?: number, successPercent?: number) => ReactNode;
  /** Style */
  style?: React.CSSProperties;
  className?: string;
}

const statusColorMap: Record<string, string> = {
  normal: "bg-sky-500",
  success: "bg-green-500",
  exception: "bg-red-500",
  active: "bg-sky-500",
};

export function Progress({
  percent = 0,
  type = "line",
  status = "normal",
  showInfo = true,
  strokeWidth,
  width: circleWidth = 80,
  trailColor,
  strokeColor,
  size = "default",
  className,
}: ProgressProps) {
  const clampedPercent = Math.min(100, Math.max(0, percent));
  const effectiveStatus =
    status === "normal" && clampedPercent >= 100 ? "success" : status;

  // When size is a number, use it as circle diameter
  const effectiveCircleWidth = typeof size === "number" ? size : circleWidth;

  if (type === "circle") {
    const sw = strokeWidth ?? 6;
    const r = (effectiveCircleWidth - sw) / 2;
    const circumference = 2 * Math.PI * r;
    const offset = circumference - (clampedPercent / 100) * circumference;
    const circleStroke =
      strokeColor ??
      (effectiveStatus === "success"
        ? "#22c55e"
        : effectiveStatus === "exception"
          ? "#ef4444"
          : "#0ea5e9");

    return (
      <div className={cn("inline-flex flex-col items-center", className)}>
        <svg width={effectiveCircleWidth} height={effectiveCircleWidth}>
          <circle
            cx={effectiveCircleWidth / 2}
            cy={effectiveCircleWidth / 2}
            r={r}
            fill="none"
            stroke={trailColor ?? "currentColor"}
            strokeWidth={sw}
            className="text-slate-200 dark:text-slate-700"
          />
          <circle
            cx={effectiveCircleWidth / 2}
            cy={effectiveCircleWidth / 2}
            r={r}
            fill="none"
            stroke={circleStroke}
            strokeWidth={sw}
            strokeLinecap="round"
            strokeDasharray={circumference}
            strokeDashoffset={offset}
            className="transition-all duration-300"
            transform={`rotate(-90 ${effectiveCircleWidth / 2} ${effectiveCircleWidth / 2})`}
          />
          {showInfo ? (
            <text
              x="50%"
              y="50%"
              textAnchor="middle"
              dominantBaseline="central"
              className="fill-[var(--text-secondary)] text-sm font-medium"
            >
              {`${Math.round(clampedPercent)}%`}
            </text>
          ) : null}
        </svg>
      </div>
    );
  }

  // Line progress
  const h = strokeWidth ?? (size === "small" ? 4 : 8);
  const showInner = showInfo && h >= 14;

  return (
    <div className={cn("flex items-center gap-2 w-full", className)}>
      <div
        className="relative flex-1 rounded-full overflow-hidden"
        style={{
          height: h,
          backgroundColor: trailColor,
        }}
      >
        <div
          className={cn(
            "bg-fill-tertiary h-full w-full rounded-full overflow-hidden",
          )}
          style={trailColor ? { backgroundColor: trailColor } : undefined}
        >
          <div
            className={cn(
              "h-full rounded-full transition-all duration-300",
              strokeColor ? undefined : statusColorMap[effectiveStatus],
              effectiveStatus === "active" &&
                "relative overflow-hidden after:absolute after:inset-0 after:bg-white/25 after:animate-[progressActive_2s_ease_infinite]",
            )}
            style={{
              width: `${clampedPercent}%`,
              ...(strokeColor ? { backgroundColor: strokeColor } : {}),
            }}
          />
        </div>
        {showInner ? (
          <span
            className="absolute inset-0 flex items-center justify-center text-[10px] font-medium leading-none text-white"
            style={{
              textShadow: "0 1px 3px rgba(0,0,0,0.8), 0 0 6px rgba(0,0,0,0.6)",
            }}
          >
            {`${Math.round(clampedPercent)}%`}
          </span>
        ) : null}
      </div>
      {showInfo && !showInner ? (
        <span
          className={cn(
            "text-xs shrink-0 min-w-[2em] text-right",
            effectiveStatus === "success"
              ? "text-green-500"
              : effectiveStatus === "exception"
                ? "text-red-500"
                : "text-fg-muted",
          )}
        >
          {`${Math.round(clampedPercent)}%`}
        </span>
      ) : null}
    </div>
  );
}
