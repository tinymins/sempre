import { AlertCircle, CheckCircle, Info, X } from "lucide-react";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { cn } from "./utils";

/* ─── Types ─── */
export type ToastType = "success" | "error" | "info" | "warning";

export interface ToastOptions {
  /** Toast content */
  content: ReactNode;
  /** Duration in seconds (0 = no auto dismiss) */
  duration?: number;
  /** Unique key for dedup */
  key?: string;
}

interface ToastItem {
  id: string;
  key?: string;
  type: ToastType;
  content: ReactNode;
  duration: number;
}

interface ToastAPI {
  success: (opts: ToastOptions | string) => void;
  error: (opts: ToastOptions | string) => void;
  info: (opts: ToastOptions | string) => void;
  warning: (opts: ToastOptions | string) => void;
  destroy: (key?: string) => void;
}

const ToastContext = createContext<ToastAPI | null>(null);

/* ─── Provider ─── */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const timers = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

  const remove = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
    const timer = timers.current.get(id);
    if (timer) {
      clearTimeout(timer);
      timers.current.delete(id);
    }
  }, []);

  const add = useCallback(
    (type: ToastType, opts: ToastOptions | string) => {
      const normalized: ToastOptions =
        typeof opts === "string" ? { content: opts } : opts;
      const duration = normalized.duration ?? 3;
      const id =
        normalized.key ??
        `toast-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

      setToasts((prev) => {
        // Dedup by key — replace existing
        if (normalized.key) {
          const existing = prev.find((t) => t.key === normalized.key);
          if (existing) {
            return prev.map((t) =>
              t.key === normalized.key
                ? { ...t, id, type, content: normalized.content, duration }
                : t,
            );
          }
        }
        return [
          ...prev,
          {
            id,
            key: normalized.key,
            type,
            content: normalized.content,
            duration,
          },
        ];
      });

      // Only set a timer if one isn't already running for this id.
      // This prevents rapid duplicate errors from indefinitely extending
      // the toast lifetime by repeatedly resetting the countdown.
      if (duration > 0 && !timers.current.has(id)) {
        const timer = setTimeout(() => remove(id), duration * 1000);
        timers.current.set(id, timer);
      }
    },
    [remove],
  );

  const destroy = useCallback((key?: string) => {
    if (key) {
      setToasts((prev) => prev.filter((t) => t.key !== key));
    } else {
      setToasts([]);
    }
  }, []);

  const api = useMemo<ToastAPI>(
    () => ({
      success: (o) => add("success", o),
      error: (o) => add("error", o),
      info: (o) => add("info", o),
      warning: (o) => add("warning", o),
      destroy,
    }),
    [add, destroy],
  );

  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);

  return (
    <ToastContext.Provider value={api}>
      {children}
      {mounted &&
        createPortal(
          <div className="fixed top-4 left-1/2 -translate-x-1/2 z-[99999] flex flex-col items-center gap-2 pointer-events-none">
            {toasts.map((toast) => (
              <ToastItem
                key={toast.id}
                toast={toast}
                onClose={() => remove(toast.id)}
              />
            ))}
          </div>,
          document.body,
        )}
    </ToastContext.Provider>
  );
}

/* ─── Toast Item ─── */
const iconMap: Record<ToastType, ReactNode> = {
  success: <CheckCircle className="h-4 w-4 text-green-500" />,
  error: <AlertCircle className="h-4 w-4 text-red-500" />,
  info: <Info className="h-4 w-4 text-blue-500" />,
  warning: <AlertCircle className="h-4 w-4 text-amber-500" />,
};

function ToastItem({
  toast,
  onClose,
}: {
  toast: ToastItem;
  onClose: () => void;
}) {
  return (
    <div
      className={cn(
        "pointer-events-auto flex items-center gap-2 rounded-lg bg-white/90 dark:bg-[rgba(15,15,25,0.9)] backdrop-blur-xl border border-black/[0.06] dark:border-white/[0.08] shadow-lg px-4 py-3 text-sm text-[var(--text-primary)] animate-[toastIn_0.2s_ease-out]",
      )}
    >
      {iconMap[toast.type]}
      <span>{toast.content}</span>
      <button
        type="button"
        className="ml-1 text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
        onClick={onClose}
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

/* ─── Hook ─── */
export function useToast(): ToastAPI {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error("useToast must be used within <ToastProvider>");
  }
  return ctx;
}
