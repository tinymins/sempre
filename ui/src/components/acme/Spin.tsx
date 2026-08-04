import { cn } from "./utils";

export interface SpinProps {
  /** Whether spinning */
  spinning?: boolean;
  /** Spin size */
  size?: "small" | "default" | "large";
  /** Tip text below spinner */
  tip?: string;
  /** Wrap children in spinning overlay */
  children?: React.ReactNode;
  className?: string;
}

const sizeMap = {
  small: "h-4 w-4",
  default: "h-8 w-8",
  large: "h-12 w-12",
};

function Spinner({ size = "default" }: { size?: SpinProps["size"] }) {
  return (
    <svg
      className={cn("animate-spin text-[var(--accent)]", sizeMap[size])}
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
  );
}

export function Spin({
  spinning = true,
  size = "default",
  tip,
  children,
  className,
}: SpinProps) {
  if (!children) {
    if (!spinning) return null;
    return (
      <div
        className={cn(
          "flex flex-col items-center justify-center gap-2",
          className,
        )}
      >
        <Spinner size={size} />
        {tip ? (
          <span className="text-sm text-[var(--text-secondary)]">{tip}</span>
        ) : null}
      </div>
    );
  }

  return (
    <div className={cn("relative", className)}>
      {children}
      {spinning ? (
        <div className="absolute inset-0 flex flex-col items-center justify-center bg-white/60 dark:bg-[rgba(10,10,15,0.6)] z-10 rounded-md">
          <Spinner size={size} />
          {tip ? (
            <span className="text-sm text-[var(--text-secondary)]">{tip}</span>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
