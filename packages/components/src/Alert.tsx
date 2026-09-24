import { AlertCircle, AlertTriangle, CheckCircle, Info, X } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "./utils";

export interface AlertProps {
  /** Alert type */
  type?: "success" | "info" | "warning" | "error";
  /** Alert message/title */
  message?: ReactNode;
  /** Alias for message (antd compat) */
  title?: ReactNode;
  /** Alert description/body */
  description?: ReactNode;
  /** Whether to show icon */
  showIcon?: boolean;
  /** Closable */
  closable?: boolean;
  /** Close callback */
  onClose?: () => void;
  /** Whether banner mode (no border, full width) */
  banner?: boolean;
  /** Custom icon */
  icon?: ReactNode;
  /** Extra action in the top-right */
  action?: ReactNode;
  className?: string;
}

const typeConfig = {
  success: {
    bg: "bg-green-50 border-green-200 dark:bg-green-950/50 dark:border-green-800",
    icon: <CheckCircle className="h-4 w-4 text-green-500" />,
    text: "text-green-800 dark:text-green-300",
  },
  info: {
    bg: "bg-blue-50 border-blue-200 dark:bg-blue-950/50 dark:border-blue-800",
    icon: <Info className="h-4 w-4 text-blue-500" />,
    text: "text-blue-800 dark:text-blue-300",
  },
  warning: {
    bg: "bg-amber-50 border-amber-200 dark:bg-amber-950/50 dark:border-amber-800",
    icon: <AlertTriangle className="h-4 w-4 text-amber-500" />,
    text: "text-amber-800 dark:text-amber-300",
  },
  error: {
    bg: "bg-red-50 border-red-200 dark:bg-red-950/50 dark:border-red-800",
    icon: <AlertCircle className="h-4 w-4 text-red-500" />,
    text: "text-red-800 dark:text-red-300",
  },
};

export function Alert({
  type = "info",
  message: messageProp,
  title,
  description,
  showIcon = false,
  closable = false,
  onClose,
  banner = false,
  icon,
  action,
  className,
}: AlertProps) {
  const config = typeConfig[type];
  const message = messageProp ?? title;

  return (
    <div
      className={cn(
        "rounded-md border p-3",
        config.bg,
        banner && "rounded-none border-x-0",
        className,
      )}
    >
      <div className="flex items-start gap-2">
        {showIcon ? (
          <span className="mt-0.5 shrink-0 [&>svg]:w-4 [&>svg]:h-4">
            {icon ?? config.icon}
          </span>
        ) : null}
        <div className="flex-1 min-w-0">
          <div className={cn("text-sm font-medium", config.text)}>
            {message}
          </div>
          {description ? (
            <div className={cn("mt-1 text-sm opacity-80", config.text)}>
              {description}
            </div>
          ) : null}
        </div>
        {action ? <div className="shrink-0 ml-2">{action}</div> : null}
        {closable ? (
          <button
            type="button"
            className={cn(
              "shrink-0 ml-2 hover:opacity-70 transition-opacity",
              config.text,
            )}
            onClick={onClose}
          >
            <X className="h-4 w-4" />
          </button>
        ) : null}
      </div>
    </div>
  );
}
