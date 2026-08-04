import type { ReactNode } from "react";
import { cn } from "./utils";

export interface BadgeProps {
  /** Badge count */
  count?: number;
  /** Show dot indicator instead of count */
  dot?: boolean;
  /** Max count (displays as max+) */
  overflowCount?: number;
  /** Show badge when count is 0 */
  showZero?: boolean;
  /** Badge color */
  color?: string;
  /** Status indicator mode */
  status?: "success" | "processing" | "default" | "error" | "warning";
  /** Status text */
  text?: ReactNode;
  /** Offset [left, top] */
  offset?: [number, number];
  /** Size */
  size?: "default" | "small";
  children?: ReactNode;
  className?: string;
  /** Custom style */
  style?: React.CSSProperties;
}

const statusColor: Record<string, string> = {
  success: "bg-green-500",
  processing: "bg-blue-500 animate-pulse",
  default: "bg-slate-400",
  error: "bg-red-500",
  warning: "bg-orange-500",
};

export function Badge({
  count,
  dot = false,
  overflowCount = 99,
  showZero = false,
  color,
  status,
  text,
  offset,
  size = "default",
  children,
  className,
}: BadgeProps) {
  // Status-only mode (no children wrapper)
  if (status && !children) {
    return (
      <span className={cn("inline-flex items-center gap-1.5", className)}>
        <span
          className={cn("h-2 w-2 rounded-full", statusColor[status])}
          style={color ? { backgroundColor: color } : undefined}
        />
        {text ? (
          <span className="text-sm text-fg-secondary">{text}</span>
        ) : null}
      </span>
    );
  }

  const showBadge = dot || (count !== undefined && (count > 0 || showZero));
  const displayCount =
    count !== undefined && count > overflowCount ? `${overflowCount}+` : count;

  // Standalone count badge (no children) — render inline
  if (showBadge && !children) {
    return (
      <span
        className={cn(
          "inline-flex items-center justify-center rounded-full",
          size === "small"
            ? "h-4 min-w-4 text-[10px] px-1"
            : "h-5 min-w-5 text-xs px-1",
          "bg-red-500 text-white font-medium leading-none",
          className,
        )}
        style={color ? { backgroundColor: color } : undefined}
      >
        {!dot ? displayCount : null}
      </span>
    );
  }

  return (
    <span className={cn("relative inline-flex", className)}>
      {children}
      {showBadge ? (
        <span
          className={cn(
            "absolute z-10 flex items-center justify-center",
            dot
              ? "h-2 w-2 rounded-full -top-0.5 -right-0.5"
              : size === "small"
                ? "h-4 min-w-4 rounded-full -top-1.5 -right-1.5 text-[10px] px-1"
                : "h-5 min-w-5 rounded-full -top-2 -right-2 text-xs px-1",
            "bg-red-500 text-white font-medium leading-none",
          )}
          style={{
            ...(color ? { backgroundColor: color } : {}),
            ...(offset ? { right: -offset[0], top: offset[1] } : {}),
          }}
        >
          {!dot ? displayCount : null}
        </span>
      ) : null}
    </span>
  );
}
