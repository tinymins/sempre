import { cn } from "./utils";

export interface DividerProps {
  /** Orientation of divider */
  type?: "horizontal" | "vertical";
  /** Whether dashed */
  dashed?: boolean;
  /** Text alignment when horizontal */
  orientation?: "left" | "center" | "right";
  /** Children — text label inside horizontal divider */
  children?: React.ReactNode;
  className?: string;
  style?: React.CSSProperties;
}

export function Divider({
  type = "horizontal",
  dashed = false,
  orientation = "center",
  children,
  className,
  style,
}: DividerProps) {
  if (type === "vertical") {
    return (
      <span
        className={cn(
          "inline-block mx-2 h-[1em] w-px align-middle",
          dashed
            ? "border-l border-dashed border-[var(--border-base)]"
            : "bg-[var(--border-base)]",
          className,
        )}
        style={style}
      />
    );
  }

  if (children) {
    return (
      <div
        className={cn(
          "flex items-center gap-3 my-4 text-sm text-fg-muted",
          className,
        )}
        style={style}
      >
        <div
          className={cn(
            "h-px flex-1",
            dashed
              ? "border-t border-dashed border-[var(--border-base)]"
              : "bg-[var(--border-base)]",
            orientation === "left" && "max-w-[5%]",
            orientation === "right" && "flex-[1]",
          )}
        />
        <span className="shrink-0">{children}</span>
        <div
          className={cn(
            "h-px flex-1",
            dashed
              ? "border-t border-dashed border-[var(--border-base)]"
              : "bg-[var(--border-base)]",
            orientation === "right" && "max-w-[5%]",
            orientation === "left" && "flex-[1]",
          )}
        />
      </div>
    );
  }

  return (
    <hr
      className={cn(
        "my-4 border-none h-px",
        dashed
          ? "border-t border-dashed border-[var(--border-base)]"
          : "bg-[var(--border-base)]",
        className,
      )}
      style={style}
    />
  );
}
