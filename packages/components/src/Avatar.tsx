import type { ReactNode } from "react";
import { cn } from "./utils";

export interface AvatarProps {
  /** Image url */
  src?: string;
  /** Alt text */
  alt?: string;
  /** Size in pixels */
  size?: number | "small" | "default" | "large";
  /** Shape */
  shape?: "circle" | "square";
  /** Icon placeholder when no src */
  icon?: ReactNode;
  /** Background color */
  style?: React.CSSProperties;
  className?: string;
  children?: ReactNode;
  onClick?: () => void;
}

const sizeNum = { small: 24, default: 32, large: 40 };

export function Avatar({
  src,
  alt,
  size = "default",
  shape = "circle",
  icon,
  style,
  className,
  children,
  onClick,
}: AvatarProps) {
  const px = typeof size === "number" ? size : sizeNum[size];
  const fontSize = Math.max(px * 0.45, 12);

  return (
    <span
      className={cn(
        "inline-flex items-center justify-center overflow-hidden bg-fill-tertiary text-fg-muted shrink-0 select-none",
        shape === "circle" ? "rounded-full" : "rounded",
        onClick && "cursor-pointer",
        className,
      )}
      style={{ width: px, height: px, fontSize, ...style }}
      role={onClick ? "button" : undefined}
      tabIndex={onClick ? 0 : undefined}
      onClick={onClick}
      onKeyDown={
        onClick
          ? (e) => {
              if (e.key === "Enter" || e.key === " ") onClick();
            }
          : undefined
      }
    >
      {src ? (
        <img src={src} alt={alt} className="h-full w-full object-cover" />
      ) : icon ? (
        icon
      ) : children ? (
        <span className="font-medium leading-none">{children}</span>
      ) : (
        <svg
          viewBox="0 0 24 24"
          className="h-[60%] w-[60%]"
          fill="currentColor"
        >
          <path d="M12 12c2.76 0 5-2.24 5-5s-2.24-5-5-5-5 2.24-5 5 2.24 5 5 5zm0 2c-3.33 0-10 1.67-10 5v2h20v-2c0-3.33-6.67-5-10-5z" />
        </svg>
      )}
    </span>
  );
}
