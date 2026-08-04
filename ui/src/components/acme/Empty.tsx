import { Inbox, PackageOpen } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "./utils";

export interface EmptyProps {
  /** Logo/image to display */
  image?: ReactNode;
  /** Use small variant */
  imageStyle?: React.CSSProperties;
  /** Description text */
  description?: ReactNode;
  /** Extra action below description */
  children?: ReactNode;
  className?: string;
}

const SIMPLE_SENTINEL = "__empty_simple__" as const;

export function Empty({
  image,
  imageStyle,
  description = "暂无数据",
  children,
  className,
}: EmptyProps) {
  const isSimple = image === SIMPLE_SENTINEL;

  const renderedImage = isSimple ? (
    <PackageOpen className="h-10 w-10 stroke-1 mb-2" />
  ) : (
    (image ?? <Inbox className="h-12 w-12 stroke-1 mb-2" />)
  );

  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center py-8 text-[var(--text-muted)]",
        className,
      )}
    >
      <div style={imageStyle}>{renderedImage}</div>
      {description !== false && description !== null ? (
        <p className="text-sm mb-2">{description}</p>
      ) : null}
      {children}
    </div>
  );
}

Empty.PRESENTED_IMAGE_SIMPLE = SIMPLE_SENTINEL;
