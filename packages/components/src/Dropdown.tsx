import {
  autoUpdate,
  FloatingPortal,
  flip,
  offset,
  type Placement,
  shift,
  useClick,
  useDismiss,
  useFloating,
  useHover,
  useInteractions,
  useRole,
  useTransitionStyles,
} from "@floating-ui/react";
import {
  cloneElement,
  isValidElement,
  type ReactElement,
  type ReactNode,
  useState,
} from "react";
import { FloatingVibrancy } from "./FloatingVibrancy";
import { cn } from "./utils";

export interface DropdownMenuItem {
  key?: string;
  label?: ReactNode;
  icon?: ReactNode;
  disabled?: boolean;
  danger?: boolean;
  type?: "divider" | "group" | "label";
  children?: DropdownMenuItem[];
  onClick?: () => void;
}

export interface DropdownMenuConfig {
  items: DropdownMenuItem[];
  onClick?: (info: { key: string }) => void;
}

export interface DropdownProps {
  /** Menu config */
  menu?: DropdownMenuConfig;
  /** Trigger mode */
  trigger?: Array<"click" | "hover" | "contextMenu">;
  /** Placement */
  placement?:
    | Placement
    | "bottomLeft"
    | "bottomRight"
    | "topLeft"
    | "topRight"
    | "bottomCenter"
    | "topCenter";
  /** Trigger element */
  children: ReactElement;
  /** Custom dropdown content instead of menu */
  dropdownRender?: (menu: ReactNode) => ReactNode;
  /** Open state controlled */
  open?: boolean;
  /** Open change callback */
  onOpenChange?: (open: boolean) => void;
  /** Disabled */
  disabled?: boolean;
  /** Arrow */
  arrow?: boolean;
  /** Class names (antd v5 compat) */
  classNames?: { root?: string };
}

function MenuList({
  items,
  onClick,
  onClose,
}: {
  items: DropdownMenuItem[];
  onClick?: (info: { key: string }) => void;
  onClose: () => void;
}) {
  return (
    <div className="py-1 min-w-[120px]">
      {items.map((item, i) => {
        if (item.type === "divider") {
          return (
            <div
              key={item.key || `d-${i}`}
              className="my-1 h-px bg-black/[0.06] dark:bg-white/[0.08]"
            />
          );
        }
        if (item.type === "label") {
          return (
            <div key={item.key ?? `label-${i}`} className="px-1">
              {item.label}
            </div>
          );
        }
        return (
          <button
            key={item.key ?? `item-${i}`}
            type="button"
            disabled={item.disabled}
            className={cn(
              "flex w-full items-center gap-2 px-3 py-1.5 text-sm text-left transition-colors cursor-pointer",
              item.danger
                ? "text-red-500 hover:bg-red-50 dark:hover:bg-red-900/40"
                : "text-[var(--text-primary)] hover:bg-black/[0.04] dark:hover:bg-white/[0.06]",
              item.disabled && "opacity-50 !cursor-not-allowed",
            )}
            onClick={() => {
              if (item.disabled) return;
              item.onClick?.();
              onClick?.({ key: item.key ?? "" });
              onClose();
            }}
          >
            {item.icon ? (
              <span className="shrink-0 inline-flex [&>svg]:w-[1em] [&>svg]:h-[1em]">
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

export function Dropdown({
  menu,
  trigger = ["hover"],
  placement: placementProp = "bottom-start",
  children,
  dropdownRender,
  open: openProp,
  onOpenChange,
  disabled = false,
  arrow: _arrow = false,
}: DropdownProps) {
  const [uncontrolledOpen, setUncontrolledOpen] = useState(false);
  const isControlled = openProp !== undefined;
  const open = isControlled ? openProp : uncontrolledOpen;

  const setOpen = (v: boolean) => {
    if (!isControlled) setUncontrolledOpen(v);
    onOpenChange?.(v);
  };

  const placementMap: Record<string, Placement> = {
    bottomLeft: "bottom-start",
    bottomRight: "bottom-end",
    topLeft: "top-start",
    topRight: "top-end",
    bottomCenter: "bottom",
    topCenter: "top",
  };
  const placement = (placementMap[placementProp] ?? placementProp) as Placement;

  const { refs, floatingStyles, context } = useFloating({
    open,
    onOpenChange: setOpen,
    placement,
    middleware: [offset(4), flip(), shift({ padding: 5 })],
    whileElementsMounted: autoUpdate,
  });

  const click = useClick(context, {
    enabled: trigger.includes("click") && !disabled,
  });
  const hover = useHover(context, {
    enabled: trigger.includes("hover") && !disabled,
    delay: { open: 75, close: 150 },
  });
  const dismiss = useDismiss(context);
  const role = useRole(context, { role: "menu" });

  const { getReferenceProps, getFloatingProps } = useInteractions([
    click,
    hover,
    dismiss,
    role,
  ]);

  /* ── enter / exit transition ── */
  const { isMounted, styles: transitionStyles } = useTransitionStyles(context, {
    duration: { open: 200, close: 150 },
    initial: { opacity: 0, transform: "scale(0.95) translateY(-4px)" },
    open: { opacity: 1, transform: "scale(1) translateY(0)" },
    close: { opacity: 0, transform: "scale(0.95) translateY(-4px)" },
  });

  const menuContent = menu ? (
    <MenuList
      items={menu.items}
      onClick={menu.onClick}
      onClose={() => setOpen(false)}
    />
  ) : null;

  return (
    <>
      {isValidElement(children)
        ? cloneElement(children as ReactElement<Record<string, unknown>>, {
            ref: refs.setReference,
            ...getReferenceProps(),
          })
        : children}
      {isMounted ? (
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
                backdropFilter: "blur(var(--window-blur, 24px))",
                WebkitBackdropFilter: "blur(var(--window-blur, 24px))",
                borderRadius: "var(--window-radius, 10px)",
              }}
              className={cn(
                "border shadow-lg ring-1 select-none overflow-hidden",
                "bg-[rgba(255,255,255,calc(var(--window-opacity,85)/100))] border-black/[0.06] ring-black/5",
                "dark:bg-[rgba(15,15,25,calc(var(--window-opacity,85)/100))] dark:border-white/[0.08] dark:shadow-black/40 dark:ring-white/5",
              )}
            >
              <FloatingVibrancy />
              <div className="relative">
                {dropdownRender ? dropdownRender(menuContent) : menuContent}
              </div>
            </div>
          </div>
        </FloatingPortal>
      ) : null}
    </>
  );
}
