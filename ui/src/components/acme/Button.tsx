import type { ButtonHTMLAttributes, ReactNode, Ref } from "react";
import { cn } from "./utils";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** Visual variant */
  variant?:
    | "primary"
    | "default"
    | "dashed"
    | "text"
    | "link"
    | "danger"
    | "unstyled";
  /** Size */
  size?: "xs" | "small" | "middle" | "large";
  /** Show loading spinner */
  loading?: boolean;
  /** Make the button full width */
  block?: boolean;
  /** Button shape */
  shape?: "default" | "circle" | "round";
  /** Icon placed before children */
  icon?: ReactNode;
  /** HTML submit type shortcut — maps to type="submit" */
  htmlType?: "button" | "submit" | "reset";
  /** Danger mode (can combine with variant) */
  danger?: boolean;
  /** Ref forwarding (React 19 style) */
  ref?: Ref<HTMLButtonElement>;
}

const variantClasses: Record<NonNullable<ButtonProps["variant"]>, string> = {
  primary:
    "bg-[var(--accent)] text-white hover:bg-[var(--accent-hover)] active:brightness-90 border-transparent shadow-sm",
  default:
    "bg-white/70 dark:bg-white/[0.05] text-[var(--text-primary,#0f172a)] border-black/[0.08] dark:border-white/[0.1] hover:text-[var(--accent)] hover:border-[var(--accent)] backdrop-blur-sm",
  dashed:
    "bg-white/70 dark:bg-white/[0.05] text-[var(--text-primary,#0f172a)] border-black/[0.08] dark:border-white/[0.1] border-dashed hover:text-[var(--accent)] hover:border-[var(--accent)] backdrop-blur-sm",
  text: "bg-transparent text-[var(--text-primary,#0f172a)] border-transparent hover:bg-black/[0.04] dark:hover:bg-white/[0.06]",
  link: "bg-transparent text-[var(--accent)] border-transparent underline hover:brightness-110 p-0 h-auto",
  danger:
    "bg-white/70 dark:bg-white/[0.05] text-red-500 border-red-300 hover:text-red-600 hover:border-red-400 dark:text-red-400 dark:border-red-500",
  unstyled: "",
};

const sizeClasses: Record<NonNullable<ButtonProps["size"]>, string> = {
  xs: "px-1.5 py-px text-[10px] h-5",
  small: "px-2 py-0.5 text-xs h-6",
  middle: "px-3 py-1 text-sm h-8",
  large: "px-4 py-1.5 text-base h-10",
};

export function Button({
  ref,
  variant = "default",
  size = "middle",
  loading = false,
  block = false,
  shape = "default",
  icon,
  htmlType,
  danger = false,
  className,
  disabled,
  children,
  type: nativeType,
  ...rest
}: ButtonProps) {
  const isDisabled = disabled || loading;

  const shapeClass =
    shape === "circle"
      ? "!rounded-full !px-0 aspect-square"
      : shape === "round"
        ? "rounded-full"
        : "rounded-md";

  return (
    <button
      ref={ref}
      type={htmlType ?? nativeType ?? "button"}
      disabled={isDisabled}
      className={cn(
        "inline-flex items-center justify-center gap-1.5 border font-medium transition-all duration-150 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)] select-none cursor-pointer",
        variantClasses[variant],
        variant !== "link" && sizeClasses[size],
        shapeClass,
        block && "w-full",
        isDisabled && "opacity-50 !cursor-not-allowed",
        danger &&
          variant === "link" &&
          "!text-red-500 hover:!text-red-600 dark:!text-red-400 dark:hover:!text-red-300",
        danger &&
          variant !== "link" &&
          "!text-red-500 !border-transparent hover:!text-red-600 hover:!bg-red-50 dark:!text-red-400 dark:hover:!text-red-300 dark:hover:!bg-red-900/20",
        className,
      )}
      {...rest}
    >
      {loading ? (
        <svg className="h-4 w-4 animate-spin" viewBox="0 0 24 24" fill="none">
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
      ) : icon ? (
        <span className="inline-flex items-center justify-center shrink-0 [&>svg]:w-[1em] [&>svg]:h-[1em]">
          {icon}
        </span>
      ) : null}
      {children}
    </button>
  );
}
