import { ChevronRight } from "lucide-react";
import { type ReactNode, useState } from "react";
import { cn } from "./utils";

export interface CollapseItem {
  key: string;
  label: ReactNode;
  children: ReactNode;
  extra?: ReactNode;
  showArrow?: boolean;
  forceRender?: boolean;
}

export interface CollapseProps {
  /** Panel items */
  items?: CollapseItem[];
  /** Active keys */
  activeKey?: string | string[];
  /** Default active keys */
  defaultActiveKey?: string | string[];
  /** Change handler */
  onChange?: (key: string | string[]) => void;
  /** Accordion mode (only one panel open) */
  accordion?: boolean;
  /** Bordered style */
  bordered?: boolean;
  /** Ghost style (transparent bg) */
  ghost?: boolean;
  /** Size */
  size?: "small" | "middle" | "large";
  className?: string;
}

export function Collapse({
  items = [],
  activeKey: activeKeyProp,
  defaultActiveKey,
  onChange,
  accordion = false,
  bordered = true,
  ghost = false,
  size = "middle",
  className,
}: CollapseProps) {
  const normalizeKeys = (v?: string | string[]): string[] =>
    v === undefined ? [] : Array.isArray(v) ? v : [v];

  const [internalKeys, setInternalKeys] = useState<string[]>(
    normalizeKeys(defaultActiveKey),
  );

  const activeKeys =
    activeKeyProp !== undefined ? normalizeKeys(activeKeyProp) : internalKeys;

  const toggle = (key: string) => {
    let next: string[];
    if (accordion) {
      next = activeKeys.includes(key) ? [] : [key];
    } else {
      next = activeKeys.includes(key)
        ? activeKeys.filter((k) => k !== key)
        : [...activeKeys, key];
    }
    if (activeKeyProp === undefined) setInternalKeys(next);
    onChange?.(accordion ? (next[0] ?? "") : next);
  };

  const padClass = {
    small: "px-3 py-1.5 text-xs",
    middle: "px-4 py-2.5 text-sm",
    large: "px-4 py-3 text-base",
  }[size];

  return (
    <div
      className={cn(
        bordered &&
          !ghost &&
          "border border-black/[0.06] dark:border-white/[0.08] rounded-lg overflow-hidden",
        className,
      )}
    >
      {items.map((item, i) => {
        const isOpen = activeKeys.includes(item.key);
        return (
          <div
            key={item.key}
            className={cn(
              i > 0 &&
                bordered &&
                "border-t border-black/[0.06] dark:border-white/[0.08]",
            )}
          >
            <button
              type="button"
              className={cn(
                "flex w-full items-center gap-2 cursor-pointer transition-colors",
                padClass,
                ghost
                  ? "hover:bg-black/[0.03] dark:hover:bg-white/[0.04]"
                  : "bg-black/[0.02] dark:bg-white/[0.03] hover:bg-black/[0.04] dark:hover:bg-white/[0.06]",
              )}
              onClick={() => toggle(item.key)}
            >
              {item.showArrow !== false && (
                <ChevronRight
                  className={cn(
                    "h-4 w-4 shrink-0 transition-transform duration-200 text-[var(--text-muted)]",
                    isOpen && "rotate-90",
                  )}
                />
              )}
              <span className="flex-1 text-left font-medium text-[var(--text-primary)]">
                {item.label}
              </span>
              {item.extra && <span className="shrink-0">{item.extra}</span>}
            </button>
            {(isOpen || item.forceRender) && (
              <div
                className={cn(
                  padClass,
                  !isOpen && "hidden",
                  "text-[var(--text-secondary)]",
                  !ghost && "bg-transparent",
                )}
              >
                {item.children}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
