import { cn } from "./utils";

export interface SkeletonProps {
  /** Show animation */
  active?: boolean;
  /** Show avatar placeholder */
  avatar?: boolean | { size?: number; shape?: "circle" | "square" };
  /** Show title placeholder */
  title?: boolean | { width?: string | number };
  /** Paragraph placeholder config */
  paragraph?:
    | boolean
    | { rows?: number; width?: Array<string | number> | string | number };
  /** Whether loading (when false, show children) */
  loading?: boolean;
  children?: React.ReactNode;
  className?: string;
}

function SkeletonBlock({
  className,
  style,
}: {
  className?: string;
  style?: React.CSSProperties;
}) {
  return (
    <div
      className={cn("h-4 bg-fill-tertiary rounded animate-pulse", className)}
      style={style}
    />
  );
}

export function Skeleton({
  active = true,
  avatar,
  title = true,
  paragraph = true,
  loading = true,
  children,
  className,
}: SkeletonProps) {
  if (!loading) return <>{children}</>;

  const paraRows =
    typeof paragraph === "object" ? (paragraph.rows ?? 3) : paragraph ? 3 : 0;
  const paraWidths =
    typeof paragraph === "object" && Array.isArray(paragraph.width)
      ? paragraph.width
      : [];

  const avatarSize =
    typeof avatar === "object" ? (avatar.size ?? 40) : avatar ? 40 : 0;
  const avatarShape =
    typeof avatar === "object" ? (avatar.shape ?? "circle") : "circle";

  return (
    <div
      className={cn("flex gap-3", !active && "[&_*]:!animate-none", className)}
    >
      {avatar ? (
        <div
          className={cn(
            "shrink-0 bg-fill-tertiary animate-pulse",
            avatarShape === "circle" ? "rounded-full" : "rounded",
          )}
          style={{ width: avatarSize, height: avatarSize }}
        />
      ) : null}
      <div className="flex-1 space-y-3">
        {title ? (
          <SkeletonBlock
            className="!h-5"
            style={{
              width: typeof title === "object" ? (title.width ?? "40%") : "40%",
            }}
          />
        ) : null}
        {Array.from({ length: paraRows }).map((_, i) => (
          <SkeletonBlock
            // biome-ignore lint/suspicious/noArrayIndexKey: skeleton rows have no unique key
            key={i}
            style={{
              width: paraWidths[i] ?? (i === paraRows - 1 ? "60%" : "100%"),
            }}
          />
        ))}
      </div>
    </div>
  );
}
