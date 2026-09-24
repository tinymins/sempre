/**
 * SettingsMenu — slide-transition menu navigator for settings panels.
 *
 * Supports arbitrary nesting:
 *   items[].items  → sub-menu (shows list, can go deeper)
 *   items[].content → leaf page (shows content, with back button)
 *
 * Controlled: pass `path` + `onPathChange` for external state persistence.
 * Uncontrolled: omit both, internal state is used.
 *
 * `path` is an array of item keys representing the navigation stack.
 * [] = show root items, ["profile"] = show profile content, etc.
 */

import { ArrowLeft, ChevronRight } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { cn } from "./utils/cn";

// ── Types ──

export interface SettingsMenuItem {
  key: string;
  icon?: React.ReactNode;
  label: string;
  /** Short description shown in the list row */
  desc?: string;
  /** Sub-menu items (branch node) */
  items?: SettingsMenuItem[];
  /** Leaf content rendered when this item is active */
  content?: React.ReactNode;
}

export interface SettingsMenuProps {
  items: SettingsMenuItem[];
  /** Controlled path. Omit for uncontrolled. */
  path?: string[];
  onPathChange?: (path: string[]) => void;
  /**
   * Label shown in the back button when at depth 1 going back to root.
   * E.g. "账户" so the button reads "← 账户".
   */
  rootLabel?: string;
  className?: string;
}

// ── Helpers ──

type ResolvedView =
  | {
      kind: "menu";
      items: SettingsMenuItem[];
      /** null means we're already at root (no back button) */
      backPath: string[] | null;
      backLabel: string | undefined;
    }
  | {
      kind: "leaf";
      content: React.ReactNode;
      backPath: string[];
      backLabel: string | undefined;
    };

function resolveView(
  rootItems: SettingsMenuItem[],
  path: string[],
  rootLabel: string | undefined,
): ResolvedView {
  if (path.length === 0) {
    return {
      kind: "menu",
      items: rootItems,
      backPath: null,
      backLabel: undefined,
    };
  }

  let currentItems = rootItems;
  const ancestorLabels: string[] = [];

  for (let i = 0; i < path.length; i++) {
    const item = currentItems.find((x) => x.key === path[i]);
    if (!item) {
      // Invalid key — fall back to root
      return {
        kind: "menu",
        items: rootItems,
        backPath: null,
        backLabel: undefined,
      };
    }

    const isLast = i === path.length - 1;
    const backPath = path.slice(0, -1);
    const backLabel =
      ancestorLabels.length > 0
        ? ancestorLabels[ancestorLabels.length - 1]
        : rootLabel;

    if (isLast) {
      if (item.items && item.items.length > 0) {
        return { kind: "menu", items: item.items, backPath, backLabel };
      }
      return {
        kind: "leaf",
        content: item.content ?? null,
        backPath,
        backLabel,
      };
    }

    ancestorLabels.push(item.label);
    currentItems = item.items ?? [];
  }

  return {
    kind: "menu",
    items: rootItems,
    backPath: null,
    backLabel: undefined,
  };
}

// ── Main component ──

export function SettingsMenu({
  items,
  path: externalPath,
  onPathChange,
  rootLabel,
  className,
}: SettingsMenuProps) {
  const isControlled = externalPath !== undefined;
  const [internalPath, setInternalPath] = useState<string[]>([]);
  const path = isControlled ? externalPath : internalPath;

  // Compute slide direction synchronously during render by comparing to prev path
  const prevRef = useRef({ key: path.join("/"), len: path.length });
  const pathKey = path.join("/");

  let slideDir: "forward" | "back" | "none" = "none";
  if (pathKey !== prevRef.current.key) {
    slideDir =
      path.length > prevRef.current.len
        ? "forward"
        : path.length < prevRef.current.len
          ? "back"
          : "none";
    prevRef.current = { key: pathKey, len: path.length };
  }

  const navigate = (newPath: string[]) => {
    if (isControlled) {
      onPathChange?.(newPath);
    } else {
      setInternalPath(newPath);
    }
  };

  const view = resolveView(items, path, rootLabel);

  return (
    <div className={cn("h-full", className)}>
      <SlidePanel pathKey={pathKey} direction={slideDir}>
        {view.kind === "menu" ? (
          <MenuList
            items={view.items}
            backPath={view.backPath}
            backLabel={view.backLabel}
            onSelect={(key) => navigate([...path, key])}
            onBack={() => navigate(view.backPath ?? [])}
          />
        ) : (
          <LeafView
            content={view.content}
            backPath={view.backPath}
            backLabel={view.backLabel}
            onBack={() => navigate(view.backPath)}
          />
        )}
      </SlidePanel>
    </div>
  );
}

// ── Slide panel (DOM-based to avoid React batch problem) ──

function SlidePanel({
  pathKey,
  direction,
  children,
}: {
  pathKey: string;
  direction: "forward" | "back" | "none";
  children: React.ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const prevKeyRef = useRef(pathKey);
  // Capture direction when pathKey changes (direction computed in parent render)
  const dirRef = useRef(direction);
  if (pathKey !== prevKeyRef.current) {
    dirRef.current = direction;
  }

  useEffect(() => {
    const el = ref.current;
    if (!el || pathKey === prevKeyRef.current) return;

    prevKeyRef.current = pathKey;
    const dir = dirRef.current;

    el.style.transition = "none";
    el.style.opacity = "0";
    el.style.transform =
      dir === "forward"
        ? "translateX(32px)"
        : dir === "back"
          ? "translateX(-32px)"
          : "translateX(0)";

    // Force reflow
    el.getBoundingClientRect();

    el.style.transition =
      "transform 220ms cubic-bezier(0.25,0.46,0.45,0.94), opacity 220ms ease-out";
    el.style.opacity = "1";
    el.style.transform = "translateX(0)";
  }, [pathKey]);

  return <div ref={ref}>{children}</div>;
}

// ── Menu list ──

function MenuList({
  items,
  backPath,
  backLabel,
  onSelect,
  onBack,
}: {
  items: SettingsMenuItem[];
  backPath: string[] | null;
  backLabel: string | undefined;
  onSelect: (key: string) => void;
  onBack: () => void;
}) {
  return (
    <div className="select-none">
      {backPath !== null && (
        <button
          type="button"
          onClick={onBack}
          className="flex items-center gap-1 mb-3 text-sm text-[var(--accent)] hover:underline cursor-pointer"
        >
          <ArrowLeft size={14} />
          {backLabel && <span>{backLabel}</span>}
        </button>
      )}
      <div className="rounded-xl border border-black/[0.06] dark:border-white/[0.08] bg-white/50 dark:bg-white/[0.03] overflow-hidden divide-y divide-black/[0.04] dark:divide-white/[0.06]">
        {items.map((item, i) => (
          <MenuItem key={item.key} item={item} index={i} onSelect={onSelect} />
        ))}
      </div>
    </div>
  );
}

function MenuItem({
  item,
  index,
  onSelect,
}: {
  item: SettingsMenuItem;
  index: number;
  onSelect: (key: string) => void;
}) {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const id = setTimeout(() => setVisible(true), index * 30);
    return () => clearTimeout(id);
  }, [index]);

  return (
    <button
      type="button"
      onClick={() => onSelect(item.key)}
      className={cn(
        "w-full flex items-center gap-3 px-4 py-3.5 text-left",
        "transition-all duration-200 ease-out cursor-pointer",
        "hover:bg-black/[0.04] dark:hover:bg-white/[0.04] group",
        visible ? "opacity-100 translate-y-0" : "opacity-0 translate-y-1",
      )}
    >
      {item.icon && (
        <span className="text-base text-fg-muted shrink-0">{item.icon}</span>
      )}
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium text-fg-primary leading-tight">
          {item.label}
        </div>
        {item.desc && (
          <div className="text-xs text-fg-muted mt-0.5 leading-tight">
            {item.desc}
          </div>
        )}
      </div>
      {(item.items || item.content !== undefined) && (
        <ChevronRight
          size={14}
          className="shrink-0 text-fg-muted group-hover:text-fg-secondary transition-colors"
        />
      )}
    </button>
  );
}

// ── Leaf content view ──

function LeafView({
  content,
  backLabel,
  onBack,
}: {
  content: React.ReactNode;
  backPath: string[];
  backLabel: string | undefined;
  onBack: () => void;
}) {
  return (
    <div>
      <button
        type="button"
        onClick={onBack}
        className="flex items-center gap-1 mb-4 text-sm text-[var(--accent)] hover:underline cursor-pointer"
      >
        <ArrowLeft size={14} />
        {backLabel && <span>{backLabel}</span>}
      </button>
      <div className="rounded-xl border border-black/[0.06] dark:border-white/[0.08] bg-white/50 dark:bg-white/[0.03] p-5">
        {content}
      </div>
    </div>
  );
}
