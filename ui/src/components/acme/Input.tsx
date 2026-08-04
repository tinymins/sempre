import { Eye, EyeOff, Search } from "lucide-react";
import {
  forwardRef,
  type InputHTMLAttributes,
  type ReactNode,
  type TextareaHTMLAttributes,
  useState,
} from "react";
import { cn } from "./utils";

/* ─── Base Input ─── */
export interface InputProps
  extends Omit<
    InputHTMLAttributes<HTMLInputElement>,
    "size" | "prefix" | "suffix"
  > {
  size?: "small" | "middle" | "large";
  /** Prefix icon/element */
  prefix?: ReactNode;
  /** Suffix icon/element */
  suffix?: ReactNode;
  /** Red border on error */
  status?: "error" | "warning";
  /** Allow clear button */
  allowClear?: boolean;
  /** Enter key handler */
  onPressEnter?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
}

const sizeMap = {
  small: "px-2 py-0.5 text-xs h-6",
  middle: "px-3 py-1.5 text-sm h-8",
  large: "px-3 py-2 text-base h-10",
};

const BaseInput = forwardRef<HTMLInputElement, InputProps>(
  (
    {
      size = "middle",
      prefix,
      suffix,
      status,
      className,
      autoComplete,
      ...rest
    },
    ref,
  ) => (
    <div
      className={cn(
        "w-full flex items-center gap-2 rounded-md border bg-[var(--input-bg)] transition-colors focus-within:border-[var(--accent)] focus-within:ring-1 focus-within:ring-[var(--accent)] dark:focus-within:border-[var(--accent)]",
        status === "error"
          ? "border-red-500"
          : status === "warning"
            ? "border-amber-500"
            : "border-black/[0.08] dark:border-white/[0.1]",
        sizeMap[size],
        className,
      )}
    >
      {prefix ? (
        <span className="text-[var(--text-muted)] shrink-0 [&>svg]:w-[1em] [&>svg]:h-[1em]">
          {prefix}
        </span>
      ) : null}
      <input
        ref={ref}
        autoComplete={autoComplete}
        className="w-full min-w-0 bg-transparent outline-none placeholder:text-[var(--text-muted)] text-inherit"
        {...rest}
      />
      {suffix ? (
        <span className="text-[var(--text-muted)] shrink-0 [&>svg]:w-[1em] [&>svg]:h-[1em]">
          {suffix}
        </span>
      ) : null}
    </div>
  ),
);
BaseInput.displayName = "Input";

/* ─── Password ─── */
export interface PasswordProps extends Omit<InputProps, "type" | "suffix"> {
  /** Visibility toggle config (antd compat) */
  visibilityToggle?:
    | boolean
    | { visible?: boolean; onVisibleChange?: (visible: boolean) => void };
  /** Custom icon render (antd compat) */
  iconRender?: (visible: boolean) => ReactNode;
}

export const Password = forwardRef<HTMLInputElement, PasswordProps>(
  ({ autoComplete, ...props }, ref) => {
    const [visible, setVisible] = useState(false);
    return (
      <BaseInput
        ref={ref}
        autoComplete={autoComplete}
        type={visible ? "text" : "password"}
        suffix={
          <button
            type="button"
            tabIndex={-1}
            className="text-[var(--text-muted)] hover:text-[var(--text-secondary)] focus:outline-none"
            onClick={() => setVisible((v) => !v)}
          >
            {visible ? (
              <EyeOff className="h-4 w-4" />
            ) : (
              <Eye className="h-4 w-4" />
            )}
          </button>
        }
        {...props}
      />
    );
  },
);
Password.displayName = "Password";

/* ─── TextArea ─── */
export interface TextAreaProps
  extends Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "size"> {
  size?: "small" | "middle" | "large";
  status?: "error" | "warning";
}

export const TextArea = forwardRef<HTMLTextAreaElement, TextAreaProps>(
  ({ size = "middle", status, className, style, ...rest }, ref) => (
    <textarea
      ref={ref}
      className={cn(
        "w-full rounded-md border bg-[var(--input-bg)] transition-colors focus:border-[var(--accent)] focus:ring-1 focus:ring-[var(--accent)] outline-none placeholder:text-[var(--text-muted)]",
        status === "error"
          ? "border-red-500"
          : status === "warning"
            ? "border-amber-500"
            : "border-black/[0.08] dark:border-white/[0.1]",
        size === "small"
          ? "px-2 py-1 text-xs"
          : size === "large"
            ? "px-3 py-2 text-base"
            : "px-3 py-1.5 text-sm",
        className,
      )}
      style={style}
      {...rest}
    />
  ),
);
TextArea.displayName = "TextArea";

/* ─── SearchInput ─── */
export interface SearchInputProps extends Omit<InputProps, "suffix"> {
  onSearch?: (value: string) => void;
}

export const SearchInput = forwardRef<HTMLInputElement, SearchInputProps>(
  ({ onSearch, onKeyDown, ...rest }, ref) => (
    <BaseInput
      ref={ref}
      suffix={<Search className="h-4 w-4" />}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          onSearch?.((e.target as HTMLInputElement).value);
        }
        onKeyDown?.(e);
      }}
      {...rest}
    />
  ),
);
SearchInput.displayName = "SearchInput";

function InputGroup({
  children,
  className,
}: {
  children?: ReactNode;
  className?: string;
}) {
  return <div className={cn("flex", className)}>{children}</div>;
}

export const Input = Object.assign(BaseInput, {
  Password,
  TextArea,
  Group: InputGroup,
});
