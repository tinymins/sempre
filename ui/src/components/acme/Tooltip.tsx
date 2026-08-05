import {
  autoUpdate,
  FloatingPortal,
  flip,
  offset,
  type Placement,
  shift,
  useDismiss,
  useFloating,
  useFocus,
  useHover,
  useInteractions,
  useRole,
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

export interface TooltipProps {
  /** Tooltip content */
  title?: ReactNode;
  /** Placement */
  placement?: Placement;
  /** Trigger element */
  children: ReactElement;
  /** Open state (controlled) */
  open?: boolean;
  /** Color preset */
  color?: string;
  /** Mouse enter delay (ms) */
  mouseEnterDelay?: number;
  /** Mouse leave delay (ms) */
  mouseLeaveDelay?: number;
}

export function Tooltip({
  title,
  placement = "top",
  children,
  open: openProp,
  color,
  mouseEnterDelay = 100,
  mouseLeaveDelay = 100,
}: TooltipProps) {
  const [isOpen, setIsOpen] = useState(false);
  const open = openProp ?? isOpen;

  const { refs, floatingStyles, context } = useFloating({
    open,
    onOpenChange: openProp === undefined ? setIsOpen : undefined,
    placement,
    middleware: [offset(6), flip(), shift({ padding: 5 })],
    whileElementsMounted: autoUpdate,
  });

  const hover = useHover(context, {
    delay: { open: mouseEnterDelay, close: mouseLeaveDelay },
  });
  const focus = useFocus(context);
  const dismiss = useDismiss(context);
  const role = useRole(context, { role: "tooltip" });

  const { getReferenceProps, getFloatingProps } = useInteractions([
    hover,
    focus,
    dismiss,
    role,
  ]);

  if (!title) return <>{children}</>;

  return (
    <>
      {isValidElement(children)
        ? cloneElement(children as ReactElement<Record<string, unknown>>, {
            ref: refs.setReference,
            ...getReferenceProps(),
          })
        : children}
      {open ? (
        <FloatingPortal>
          <div
            ref={refs.setFloating}
            style={floatingStyles}
            className={cn(
              "z-[9999] max-w-xs overflow-hidden break-all rounded px-2 py-1 text-xs shadow-lg",
              color
                ? color
                : "bg-[rgba(255,255,255,calc(var(--window-opacity,85)/100))] dark:bg-[rgba(15,15,25,calc(var(--window-opacity,85)/100))] text-[var(--text-primary)] backdrop-blur-xl border border-black/[0.06] dark:border-white/[0.08]",
            )}
            {...getFloatingProps()}
          >
            <FloatingVibrancy />
            <span className="relative">{title}</span>
          </div>
        </FloatingPortal>
      ) : null}
    </>
  );
}
