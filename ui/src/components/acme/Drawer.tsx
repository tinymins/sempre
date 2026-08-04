import { X } from "lucide-react";
import {
  type CSSProperties,
  type ReactNode,
  type RefObject,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { pushEscapeHandler, removeEscapeHandler } from "./escape-stack";
import { ModalContainerContext } from "./Modal";
import { cn } from "./utils";

export interface DrawerProps {
  /** Whether visible */
  open?: boolean;
  /** Title */
  title?: ReactNode;
  /** Placement */
  placement?: "left" | "right" | "top" | "bottom";
  /** Width (for left/right) */
  width?: number | string;
  /** Height (for top/bottom) */
  height?: number | string;
  /** Close callback */
  onClose?: () => void;
  /** Close on mask click */
  maskClosable?: boolean;
  /** Closable */
  closable?: boolean;
  /** Z index */
  zIndex?: number;
  /** Keyboard closable */
  keyboard?: boolean;
  /** Extra content in header */
  extra?: ReactNode;
  /** Custom footer */
  footer?: ReactNode;
  /** Body style */
  bodyStyle?: CSSProperties;
  /** Ant Design v5 styles API */
  styles?: {
    body?: CSSProperties;
    wrapper?: CSSProperties;
    header?: CSSProperties;
    root?: CSSProperties;
  };
  /** Destroy on close */
  destroyOnClose?: boolean;
  /** Explicit container — overrides ModalContainerContext */
  container?: RefObject<HTMLElement | null>;
  className?: string;
  children?: ReactNode;
}

const DURATION = 300;

const slideTransforms: Record<string, { hidden: string; visible: string }> = {
  left: { hidden: "translateX(-100%)", visible: "translateX(0)" },
  right: { hidden: "translateX(100%)", visible: "translateX(0)" },
  top: { hidden: "translateY(-100%)", visible: "translateY(0)" },
  bottom: { hidden: "translateY(100%)", visible: "translateY(0)" },
};

export function Drawer({
  open = false,
  title,
  placement = "right",
  width = 378,
  height = 378,
  onClose,
  maskClosable = true,
  closable = true,
  zIndex = 1000,
  keyboard = true,
  extra,
  footer,
  bodyStyle,
  styles,
  destroyOnClose = false,
  container,
  className,
  children,
}: DrawerProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const ctxContainer = useContext(ModalContainerContext);
  const resolvedContainer = container ?? ctxContainer;
  const [mounted, setMounted] = useState(open);
  const [visible, setVisible] = useState(false);
  const rafRef = useRef(0);

  // Open → mount the DOM; Close → start exit animation
  useEffect(() => {
    if (open) {
      setMounted(true);
    } else {
      setVisible(false);
    }
  }, [open]);

  // After mounted + open, trigger enter animation
  useEffect(() => {
    if (!mounted || !open) return;
    // Reset to hidden (handles quick re-open where visible might still be true)
    setVisible(false);
    rafRef.current = requestAnimationFrame(() => {
      // Force reflow so the browser registers the hidden position
      panelRef.current?.getBoundingClientRect();
      setVisible(true);
    });
    return () => cancelAnimationFrame(rafRef.current);
  }, [mounted, open]);

  // Unmount after exit transition completes (ignore bubbled events from children)
  const handleTransitionEnd = useCallback(
    (e: React.TransitionEvent) => {
      if (e.target !== e.currentTarget || e.propertyName !== "transform")
        return;
      if (!open) setMounted(false);
    },
    [open],
  );

  // Safety: unmount if transitionEnd doesn't fire
  useEffect(() => {
    if (!open && mounted && !visible) {
      const timer = setTimeout(() => setMounted(false), DURATION + 50);
      return () => clearTimeout(timer);
    }
  }, [open, mounted, visible]);

  // Uses global escape stack so only the topmost overlay closes
  useEffect(() => {
    if (!open || !keyboard) return;
    const handler = () => onClose?.();
    pushEscapeHandler(handler);
    return () => removeEscapeHandler(handler);
  }, [open, keyboard, onClose]);

  // Body scroll lock — skip when rendering inside a container
  useEffect(() => {
    if (open && !resolvedContainer?.current) {
      const prev = document.body.style.overflow;
      document.body.style.overflow = "hidden";
      return () => {
        document.body.style.overflow = prev;
      };
    }
  }, [open, resolvedContainer]);

  if (!mounted && destroyOnClose) return null;
  if (!mounted) return null;

  const isHorizontal = placement === "left" || placement === "right";

  const slide = slideTransforms[placement];

  const panelStyle: CSSProperties = {
    ...(isHorizontal ? { width, height: "100%" } : { height, width: "100%" }),
    transform: visible ? slide.visible : slide.hidden,
    transition: `transform ${DURATION}ms cubic-bezier(0.2, 0, 0, 1)`,
  };

  const positionClass = {
    left: "left-0 top-0",
    right: "right-0 top-0",
    top: "top-0 left-0",
    bottom: "bottom-0 left-0",
  }[placement];

  const isInline = !!resolvedContainer?.current;
  const portalTarget = resolvedContainer?.current ?? document.body;

  return createPortal(
    <div
      className={isInline ? "absolute inset-0" : "fixed inset-0"}
      style={{ zIndex }}
    >
      {/* Mask */}
      <div
        className="absolute inset-0 bg-black/45 backdrop-blur-sm"
        style={{
          opacity: visible ? 1 : 0,
          transition: `opacity ${DURATION}ms ease`,
        }}
        role="presentation"
        onClick={maskClosable ? onClose : undefined}
      />
      {/* Panel */}
      <div
        ref={panelRef}
        className={cn(
          "absolute shadow-2xl flex flex-col",
          "bg-white/85 dark:bg-[rgba(15,15,25,0.85)] backdrop-blur-xl border-black/[0.06] dark:border-white/[0.08]",
          positionClass,
          className,
        )}
        style={{ ...panelStyle, ...styles?.wrapper }}
        role="presentation"
        onClick={(e) => e.stopPropagation()}
        onTransitionEnd={handleTransitionEnd}
      >
        {/* Header */}
        {(title || closable) && (
          <div
            className="flex items-center justify-between px-6 py-4 border-b border-black/[0.06] dark:border-white/[0.08] shrink-0"
            style={styles?.header}
          >
            <h3 className="text-base font-semibold text-[var(--text-primary)] m-0">
              {title}
            </h3>
            <div className="flex items-center gap-2">
              {extra}
              {closable ? (
                <button
                  type="button"
                  className="text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors"
                  onClick={onClose}
                >
                  <X className="h-5 w-5" />
                </button>
              ) : null}
            </div>
          </div>
        )}
        {/* Body */}
        <div
          className="flex-1 overflow-y-auto px-6 py-4"
          style={{
            scrollbarWidth: "thin",
            scrollbarColor: "rgba(128,128,128,0.4) transparent",
            ...bodyStyle,
            ...styles?.body,
          }}
        >
          {children}
        </div>
        {/* Footer */}
        {footer ? (
          <div className="px-6 py-4 border-t border-black/[0.06] dark:border-white/[0.08] shrink-0">
            {footer}
          </div>
        ) : null}
      </div>
    </div>,
    portalTarget,
  );
}
