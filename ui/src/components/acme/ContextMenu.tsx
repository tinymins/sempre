import {
  FloatingPortal,
  flip,
  offset,
  safePolygon,
  shift,
  useClientPoint,
  useDismiss,
  useFloating,
  useHover,
  useInteractions,
  useRole,
  useTransitionStyles,
} from "@floating-ui/react";
import { ChevronRight } from "lucide-react";
import {
  createContext,
  type ReactNode,
  useCallback,
  useMemo,
  useRef,
  useState,
} from "react";
import { FloatingVibrancy } from "./FloatingVibrancy";
import { cn } from "./utils";

/* ─── Types ─── */

export interface ContextMenuItem {
  key?: string;
  label?: ReactNode;
  icon?: ReactNode;
  disabled?: boolean;
  danger?: boolean;
  /** Divider or group title */
  type?: "divider" | "group";
  onClick?: () => void;
  /** Custom content rendered in a hover-triggered submenu panel to the right */
  submenuContent?: ReactNode;
}

export interface ContextMenuProps {
  /** Menu items */
  items: ContextMenuItem[];
  /** Trigger area */
  children: ReactNode;
  /** Extra class on container */
  className?: string;
}

/* ─── ActiveDescendantContext ───
 *
 * State-driven keep-alive for cascading hover menus across FloatingPortal
 * boundaries. Each submenu level provides this context. Children call
 * reportOpen/reportClose when their floating panels open/close. The parent
 * blocks its own close while any child is open, and re-evaluates when the
 * last child closes by checking whether the pointer is still inside its DOM.
 */

export interface ActiveDescendantControls {
  reportOpen: (id: string) => void;
  reportClose: (id: string) => void;
}

export const ActiveDescendantContext = createContext<ActiveDescendantControls>({
  reportOpen: () => {},
  reportClose: () => {},
});

/* ─── Submenu item — hover-triggered panel to the right ─── */

function SubmenuItem({ item }: { item: ContextMenuItem }) {
  const [open, setOpen] = useState(false);

  // Pointer tracking: is mouse physically inside our reference OR floating?
  const isPointerInsideRef = useRef(false);
  // Which direct children have their floating panels open?
  const openChildIds = useRef(new Set<string>());

  const myControls = useMemo<ActiveDescendantControls>(
    () => ({
      reportOpen: (childId: string) => {
        openChildIds.current.add(childId);
      },
      reportClose: (childId: string) => {
        openChildIds.current.delete(childId);
        // Last child closed — close ourselves if mouse isn't inside us
        if (openChildIds.current.size === 0 && !isPointerInsideRef.current) {
          setOpen(false);
        }
      },
    }),
    [],
  );

  const handleOpenChange = useCallback((v: boolean) => {
    if (!v && openChildIds.current.size > 0) {
      // A descendant portal is open — don't close
      return;
    }
    setOpen(v);
  }, []);

  const { refs, floatingStyles, context } = useFloating({
    open,
    onOpenChange: handleOpenChange,
    placement: "right-start",
    middleware: [
      offset({ mainAxis: 4, crossAxis: -4 }),
      flip({ padding: 8 }),
      shift({ padding: 8 }),
    ],
  });

  const hover = useHover(context, {
    delay: { open: 0, close: 75 },
    move: false,
    handleClose: safePolygon(),
  });
  const { getReferenceProps, getFloatingProps } = useInteractions([hover]);

  const { isMounted, styles: transitionStyles } = useTransitionStyles(context, {
    duration: 50,
    initial: { opacity: 0 },
  });

  return (
    <ActiveDescendantContext.Provider value={myControls}>
      <button
        ref={refs.setReference}
        type="button"
        disabled={item.disabled}
        className={cn(
          "flex w-full items-center gap-2.5 px-3 py-1.5 text-sm text-left",
          "transition-colors duration-100 cursor-default select-none",
          item.danger
            ? "text-red-500 hover:bg-red-50 dark:hover:bg-red-900/40"
            : "text-[var(--text-primary)] hover:bg-black/[0.04] dark:hover:bg-white/[0.06]",
          item.disabled && "opacity-40 !cursor-not-allowed",
          open && !item.danger && "bg-black/[0.04] dark:bg-white/[0.06]",
        )}
        {...getReferenceProps({
          onPointerEnter: () => {
            isPointerInsideRef.current = true;
          },
          onPointerLeave: () => {
            isPointerInsideRef.current = false;
          },
        })}
      >
        {item.icon ? (
          <span className="shrink-0 inline-flex [&>svg]:w-[1em] [&>svg]:h-[1em] opacity-70">
            {item.icon}
          </span>
        ) : null}
        <span className="flex-1">{item.label}</span>
        <ChevronRight size={12} className="opacity-40 shrink-0" />
      </button>

      {isMounted && (
        <FloatingPortal>
          <div
            ref={refs.setFloating}
            style={floatingStyles}
            className="z-[9999]"
            {...getFloatingProps({
              onPointerEnter: () => {
                isPointerInsideRef.current = true;
              },
              onPointerLeave: () => {
                isPointerInsideRef.current = false;
              },
            })}
          >
            <div
              style={{
                ...transitionStyles,
                borderRadius: "var(--window-radius, 12px)",
                backdropFilter: "blur(var(--window-blur, 24px))",
                WebkitBackdropFilter: "blur(var(--window-blur, 24px))",
              }}
              className={cn(
                "min-w-[200px] border shadow-2xl overflow-hidden select-none",
                "bg-[rgba(255,255,255,calc(var(--window-opacity,85)/100))] border-black/[0.07] ring-1 ring-black/5",
                "dark:bg-[rgba(18,18,28,calc(var(--window-opacity,85)/100))] dark:border-white/[0.09] dark:ring-white/[0.06] dark:shadow-black/60",
              )}
            >
              <FloatingVibrancy />
              {item.submenuContent}
            </div>
          </div>
        </FloatingPortal>
      )}
    </ActiveDescendantContext.Provider>
  );
}

/* ─── Menu item list (shared between component & hook) ─── */

function MenuItemList({
  items,
  onClose,
}: {
  items: ContextMenuItem[];
  onClose: () => void;
}) {
  return (
    <div className="relative py-1.5">
      {items.map((item, i) => {
        if (item.type === "divider") {
          return (
            <div
              key={item.key ?? `d-${i}`}
              className="my-1 mx-2 h-px bg-black/[0.06] dark:bg-white/[0.08]"
            />
          );
        }
        if (item.type === "group") {
          return (
            <div
              key={item.key ?? `g-${i}`}
              className="px-3 py-1 text-xs font-medium text-[var(--text-tertiary)] select-none"
            >
              {item.label}
            </div>
          );
        }
        if (item.submenuContent) {
          return <SubmenuItem key={item.key ?? `item-${i}`} item={item} />;
        }
        return (
          <button
            key={item.key ?? `item-${i}`}
            type="button"
            disabled={item.disabled}
            className={cn(
              "flex w-full items-center gap-2.5 px-3 py-1.5 text-sm text-left",
              "transition-colors duration-100 cursor-pointer select-none",
              item.danger
                ? "text-red-500 hover:bg-red-50 dark:hover:bg-red-900/40"
                : "text-[var(--text-primary)] hover:bg-black/[0.04] dark:hover:bg-white/[0.06]",
              item.disabled && "opacity-40 !cursor-not-allowed",
            )}
            onClick={() => {
              if (item.disabled) return;
              item.onClick?.();
              onClose();
            }}
          >
            {item.icon ? (
              <span className="shrink-0 inline-flex [&>svg]:w-[1em] [&>svg]:h-[1em] opacity-70">
                {item.icon}
              </span>
            ) : null}
            <span className="flex-1">{item.label}</span>
          </button>
        );
      })}
    </div>
  );
}

/* ─── Floating menu panel (shared between component & hook) ─── */

function FloatingMenuPanel({
  items,
  open,
  point,
  onOpenChange,
  placement,
}: {
  items: ContextMenuItem[];
  open: boolean;
  point: { x: number; y: number };
  onOpenChange: (v: boolean) => void;
  placement: "right-start" | "right-end";
}) {
  const { refs, floatingStyles, context } = useFloating({
    open,
    onOpenChange,
    placement,
    middleware: [offset(0), flip({ padding: 8 }), shift({ padding: 8 })],
  });

  const clientPoint = useClientPoint(context, {
    enabled: open,
    x: point.x,
    y: point.y,
  });
  const dismiss = useDismiss(context, { ancestorScroll: true });
  const role = useRole(context, { role: "menu" });

  const { getFloatingProps } = useInteractions([clientPoint, dismiss, role]);

  const { isMounted, styles: transitionStyles } = useTransitionStyles(context, {
    duration: { open: 150, close: 100 },
    initial: { opacity: 0, transform: "scale(0.95)" },
    open: { opacity: 1, transform: "scale(1)" },
    close: { opacity: 0, transform: "scale(0.95)" },
  });

  if (!isMounted) return null;

  return (
    <FloatingPortal>
      <div
        ref={refs.setFloating}
        style={floatingStyles}
        className="z-[9999]"
        {...getFloatingProps()}
      >
        <div
          style={{
            ...transitionStyles,
            borderRadius: "var(--window-radius, 12px)",
            backdropFilter: "blur(var(--window-blur, 24px))",
            WebkitBackdropFilter: "blur(var(--window-blur, 24px))",
          }}
          className={cn(
            "min-w-[160px] border shadow-2xl overflow-hidden select-none",
            "bg-[rgba(255,255,255,calc(var(--window-opacity,85)/100))] border-black/[0.07] ring-1 ring-black/5",
            "dark:bg-[rgba(18,18,28,calc(var(--window-opacity,85)/100))] dark:border-white/[0.09] dark:ring-white/[0.06] dark:shadow-black/60",
          )}
        >
          <FloatingVibrancy />
          <MenuItemList items={items} onClose={() => onOpenChange(false)} />
        </div>
      </div>
    </FloatingPortal>
  );
}

/* ─── Main component ─── */

export function ContextMenu({ items, children, className }: ContextMenuProps) {
  const [open, setOpen] = useState(false);
  const pointRef = useRef({ x: 0, y: 0 });

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    pointRef.current = { x: e.clientX, y: e.clientY };
    setOpen(true);
  }, []);

  return (
    <div
      className={cn("contents", className)}
      onContextMenu={handleContextMenu}
    >
      {children}
      <FloatingMenuPanel
        items={items}
        open={open}
        point={pointRef.current}
        onOpenChange={setOpen}
        placement="right-start"
      />
    </div>
  );
}

/* ─── Hook variant for imperative / table-row usage ─── */

interface OpenAtOptions {
  /** When true, y is treated as the bottom edge — menu grows upward */
  bottomAnchor?: boolean;
}

interface ContextMenuTrigger {
  /**
   * Call from onContextMenu handler.
   * Pass items dynamically so each row can have its own menu.
   */
  open: (e: React.MouseEvent, items: ContextMenuItem[]) => void;
  /** Open at explicit coordinates (e.g. anchored to an element). */
  openAt: (
    x: number,
    y: number,
    items: ContextMenuItem[],
    options?: OpenAtOptions,
  ) => void;
  /** Menu portal — render this once near the root of your component */
  contextMenu: ReactNode;
}

export function useContextMenu(): ContextMenuTrigger {
  const [isOpen, setIsOpen] = useState(false);
  const [items, setItems] = useState<ContextMenuItem[]>([]);
  const [point, setPoint] = useState({ x: 0, y: 0 });
  const [placement, setPlacement] = useState<"right-start" | "right-end">(
    "right-start",
  );

  const open = useCallback(
    (e: React.MouseEvent, nextItems: ContextMenuItem[]) => {
      e.preventDefault();
      e.stopPropagation();
      setItems(nextItems);
      setPoint({ x: e.clientX, y: e.clientY });
      setPlacement("right-start");
      setIsOpen(true);
    },
    [],
  );

  const openAt = useCallback(
    (
      x: number,
      y: number,
      nextItems: ContextMenuItem[],
      options?: OpenAtOptions,
    ) => {
      setItems(nextItems);
      setPoint({ x, y });
      setPlacement(options?.bottomAnchor ? "right-end" : "right-start");
      setIsOpen(true);
    },
    [],
  );

  const contextMenu = useMemo(
    () => (
      <FloatingMenuPanel
        items={items}
        open={isOpen}
        point={point}
        onOpenChange={setIsOpen}
        placement={placement}
      />
    ),
    [items, isOpen, point, placement],
  );

  return { open, openAt, contextMenu };
}
