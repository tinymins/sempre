import {
  autoUpdate,
  FloatingPortal,
  flip,
  offset,
  type Placement,
  shift,
  size,
  useClick,
  useDismiss,
  useFloating,
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

export interface PopoverProps {
  /** Popover title */
  title?: ReactNode;
  /** Popover body content */
  content?: ReactNode;
  /** Placement */
  placement?:
    | Placement
    | "bottomLeft"
    | "bottomRight"
    | "topLeft"
    | "topRight"
    | "bottomCenter"
    | "topCenter";
  /** Trigger mode */
  trigger?: "click" | "hover";
  /** Trigger element */
  children: ReactElement;
  /** Open state controlled */
  open?: boolean;
  /** Open change callback */
  onOpenChange?: (open: boolean) => void;
  /** Override the floating panel className (replaces default styling) */
  popupClassName?: string;
  /** Override the content wrapper className */
  contentClassName?: string;
  /** Constrain height to fit within the viewport */
  fitViewport?: boolean;
}

export function Popover({
  title,
  content,
  placement: placementProp = "top",
  trigger = "hover",
  children,
  open: openProp,
  onOpenChange,
  popupClassName,
  contentClassName,
  fitViewport,
}: PopoverProps) {
  const [uncontrolledOpen, setUncontrolledOpen] = useState(false);
  const open = openProp ?? uncontrolledOpen;
  const setOpen = (v: boolean) => {
    if (openProp === undefined) setUncontrolledOpen(v);
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

  const middleware = [offset(6), flip(), shift({ padding: 5 })];
  if (fitViewport) {
    middleware.push(
      size({
        padding: 16,
        apply({ availableHeight, elements }) {
          Object.assign(elements.floating.style, {
            maxHeight: `${availableHeight}px`,
            overflowY: "auto",
          });
        },
      }),
    );
  }

  const { refs, floatingStyles, context } = useFloating({
    open,
    onOpenChange: setOpen,
    placement,
    middleware,
    whileElementsMounted: autoUpdate,
  });

  const click = useClick(context, { enabled: trigger === "click" });
  const hover = useHover(context, {
    enabled: trigger === "hover",
    delay: { open: 75, close: 150 },
  });
  const dismiss = useDismiss(context);
  const role = useRole(context);

  const { getReferenceProps, getFloatingProps } = useInteractions([
    click,
    hover,
    dismiss,
    role,
  ]);

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
            style={{
              ...floatingStyles,
              backdropFilter: "blur(var(--window-blur, 24px))",
              WebkitBackdropFilter: "blur(var(--window-blur, 24px))",
              borderRadius: "var(--window-radius, 10px)",
            }}
            className={
              popupClassName ??
              cn(
                "z-[9999] overflow-hidden bg-[rgba(255,255,255,calc(var(--window-opacity,85)/100))] dark:bg-[rgba(15,15,25,calc(var(--window-opacity,85)/100))] border border-black/[0.06] dark:border-white/[0.08] shadow-lg p-3 max-w-xs",
              )
            }
            {...getFloatingProps()}
          >
            <FloatingVibrancy />
            {title ? (
              <div className="relative font-medium text-sm text-[var(--text-primary)] mb-2">
                {title}
              </div>
            ) : null}
            {content ? (
              <div
                className={
                  contentClassName != null
                    ? cn("relative", contentClassName)
                    : "relative text-sm text-[var(--text-secondary)]"
                }
              >
                {content}
              </div>
            ) : null}
          </div>
        </FloatingPortal>
      ) : null}
    </>
  );
}
