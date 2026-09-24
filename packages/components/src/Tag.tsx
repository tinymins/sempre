import { X } from "lucide-react";
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./utils";

const colorMap: Record<string, string> = {
  default:
    "bg-fill-tertiary text-fg-secondary border-border-base dark:bg-white/[0.06] dark:border-white/[0.1]",
  blue: "bg-blue-50 text-blue-600 border-blue-200 dark:bg-sky-500/[0.12] dark:text-sky-400 dark:border-sky-400/[0.15]",
  green:
    "bg-green-50 text-green-600 border-green-200 dark:bg-emerald-500/[0.12] dark:text-emerald-500 dark:border-emerald-400/[0.15]",
  red: "bg-red-50 text-red-600 border-red-200 dark:bg-red-500/[0.12] dark:text-red-400 dark:border-red-400/[0.15]",
  orange:
    "bg-orange-50 text-orange-600 border-orange-200 dark:bg-orange-500/[0.12] dark:text-orange-400 dark:border-orange-400/[0.15]",
  gold: "bg-amber-50 text-amber-600 border-amber-200 dark:bg-amber-500/[0.12] dark:text-amber-400 dark:border-amber-400/[0.15]",
  yellow:
    "bg-yellow-50 text-yellow-600 border-yellow-200 dark:bg-yellow-500/[0.12] dark:text-yellow-400 dark:border-yellow-400/[0.15]",
  cyan: "bg-cyan-50 text-cyan-600 border-cyan-200 dark:bg-cyan-500/[0.12] dark:text-cyan-400 dark:border-cyan-400/[0.15]",
  purple:
    "bg-purple-50 text-purple-600 border-purple-200 dark:bg-purple-500/[0.12] dark:text-purple-400 dark:border-purple-400/[0.15]",
  magenta:
    "bg-pink-50 text-pink-600 border-pink-200 dark:bg-pink-500/[0.12] dark:text-pink-400 dark:border-pink-400/[0.15]",
  volcano:
    "bg-orange-50 text-orange-700 border-orange-300 dark:bg-orange-500/[0.12] dark:text-orange-400 dark:border-orange-400/[0.15]",
  lime: "bg-lime-50 text-lime-600 border-lime-200 dark:bg-lime-500/[0.12] dark:text-lime-400 dark:border-lime-400/[0.15]",
  geekblue:
    "bg-indigo-50 text-indigo-600 border-indigo-200 dark:bg-indigo-500/[0.12] dark:text-indigo-400 dark:border-indigo-400/[0.15]",
  success:
    "bg-green-50 text-green-600 border-green-200 dark:bg-emerald-500/[0.12] dark:text-emerald-500 dark:border-emerald-400/[0.15]",
  processing:
    "bg-blue-50 text-blue-600 border-blue-200 dark:bg-sky-500/[0.12] dark:text-sky-400 dark:border-sky-400/[0.15]",
  error:
    "bg-red-50 text-red-600 border-red-200 dark:bg-red-500/[0.12] dark:text-red-400 dark:border-red-400/[0.15]",
  warning:
    "bg-orange-50 text-orange-600 border-orange-200 dark:bg-amber-500/[0.12] dark:text-yellow-400 dark:border-amber-400/[0.15]",
};

const borderlessColorMap: Record<string, string> = {
  default: "bg-fill-tertiary text-fg-secondary dark:bg-white/[0.06]",
  blue: "bg-blue-50 text-blue-600 dark:bg-sky-500/[0.12] dark:text-sky-400",
  green:
    "bg-green-50 text-green-600 dark:bg-emerald-500/[0.12] dark:text-emerald-500",
  red: "bg-red-50 text-red-600 dark:bg-red-500/[0.12] dark:text-red-400",
  orange:
    "bg-orange-50 text-orange-600 dark:bg-orange-500/[0.12] dark:text-orange-400",
  gold: "bg-amber-50 text-amber-600 dark:bg-amber-500/[0.12] dark:text-amber-400",
  geekblue:
    "bg-indigo-50 text-indigo-600 dark:bg-indigo-500/[0.12] dark:text-indigo-400",
  purple:
    "bg-purple-50 text-purple-600 dark:bg-purple-500/[0.12] dark:text-purple-400",
  cyan: "bg-cyan-50 text-cyan-600 dark:bg-cyan-500/[0.12] dark:text-cyan-400",
  success:
    "bg-green-50 text-green-600 dark:bg-emerald-500/[0.12] dark:text-emerald-500",
  processing:
    "bg-blue-50 text-blue-600 dark:bg-sky-500/[0.12] dark:text-sky-400",
  error: "bg-red-50 text-red-600 dark:bg-red-500/[0.12] dark:text-red-400",
  warning:
    "bg-orange-50 text-orange-600 dark:bg-amber-500/[0.12] dark:text-yellow-400",
};

export interface TagProps
  extends Omit<HTMLAttributes<HTMLSpanElement>, "color"> {
  /** Preset color name or CSS color string */
  color?: string;
  /** Size */
  size?: "default" | "small";
  /** Closable tag */
  closable?: boolean;
  /** Close callback */
  onClose?: (e: React.MouseEvent) => void;
  /** Show border */
  bordered?: boolean;
  /** Icon before text */
  icon?: ReactNode;
}

export function Tag({
  color = "default",
  size = "default",
  closable = false,
  onClose,
  bordered = true,
  icon,
  className,
  children,
  ...rest
}: TagProps) {
  const isPreset = color in colorMap;
  const preset = bordered
    ? (colorMap[color] ?? colorMap.default)
    : (borderlessColorMap[color] ?? borderlessColorMap.default);

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-lg font-medium whitespace-nowrap dark:backdrop-blur-sm",
        size === "small"
          ? "px-1.5 py-px text-[10px] leading-4"
          : "px-2 py-0.5 text-xs",
        isPreset ? preset : undefined,
        bordered && isPreset ? "border" : undefined,
        className,
      )}
      style={
        !isPreset
          ? { backgroundColor: `${color}15`, color, borderColor: color }
          : undefined
      }
      {...rest}
    >
      {icon ? (
        <span className="inline-flex items-center shrink-0 [&>svg]:w-[1em] [&>svg]:h-[1em]">
          {icon}
        </span>
      ) : null}
      {children}
      {closable ? (
        <button
          type="button"
          className="inline-flex items-center cursor-pointer hover:opacity-70 ml-0.5 -mr-0.5"
          onClick={(e) => {
            e.stopPropagation();
            onClose?.(e);
          }}
        >
          <X className="h-3 w-3" />
        </button>
      ) : null}
    </span>
  );
}
