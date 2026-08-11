import { X } from "lucide-react";
import {
  createContext,
  type ReactNode,
  type RefObject,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { createRoot } from "react-dom/client";
import { Button } from "./Button";
import { pushEscapeHandler, removeEscapeHandler } from "./escape-stack";
import { cn } from "./utils";

/* ─── Modal container context ─── */
import type { ConfirmConfig, ModalProps } from "./Modal.types";
import { SIZE_CONFIG, THIN_SCROLLBAR } from "./Modal.styles";

export const ModalContainerContext =
  createContext<RefObject<HTMLElement | null> | null>(null);

/* ─── Active window container tracking (for Modal.confirm) ─── */
let activeModalContainerRef: RefObject<HTMLElement | null> | null = null;

/** Called by FloatingWindow on pointer-down so that Modal.confirm renders inside the active window. */
export function setActiveModalContainer(
  ref: RefObject<HTMLElement | null> | null,
) {
  activeModalContainerRef = ref;
}

/* ─── ScaledModal size presets ─── */

export function Modal({
  open = false,
  title,
  okText = "OK",
  cancelText = "Cancel",
  okButtonProps,
  cancelButtonProps,
  onOk,
  onCancel,
  extra,
  footer,
  width: widthProp,
  size = "default",
  maskClosable = true,
  keyboard = true,
  closable = true,
  destroyOnClose: destroyOnCloseProp,
  destroyOnHidden,
  confirmLoading = false,
  zIndex = 1000,
  bodyStyle,
  styles,
  style,
  className,
  wrapClassName,
  centered = false,
  afterOpenChange,
  container,
  children,
}: ModalProps) {
  const destroyOnClose = destroyOnCloseProp ?? destroyOnHidden ?? false;
  const contentRef = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const ctxContainer = useContext(ModalContainerContext);
  const resolvedContainer = container ?? ctxContainer;

  /* Track whether mousedown started on the mask itself (not on dialog content) */
  const mouseDownOnMask = useRef(false);

  /* ─── Animation state ─── */
  const ANIM_DURATION = 200;
  const [visible, setVisible] = useState(false);
  const [animClass, setAnimClass] = useState(false);
  const rafRef = useRef<number>(0);

  // Freeze children during exit animation so content doesn't vanish before fade-out
  const frozenChildrenRef = useRef<ReactNode>(children);
  if (open) {
    frozenChildrenRef.current = children;
  }
  const renderedChildren = open ? children : frozenChildrenRef.current;

  useEffect(() => {
    if (open) {
      setVisible(true);
      // Double rAF to ensure DOM is painted before adding transition class
      rafRef.current = requestAnimationFrame(() => {
        rafRef.current = requestAnimationFrame(() => {
          setAnimClass(true);
        });
      });
      return () => cancelAnimationFrame(rafRef.current);
    }
    // Closing: remove anim class, wait for transition, then unmount
    setAnimClass(false);
    const timer = setTimeout(() => {
      setVisible(false);
    }, ANIM_DURATION);
    return () => clearTimeout(timer);
  }, [open]);

  // afterOpenChange
  const afterOpenChangeRef = useRef(afterOpenChange);
  const wasVisibleRef = useRef(false);
  afterOpenChangeRef.current = afterOpenChange;
  useEffect(() => {
    if (visible) {
      wasVisibleRef.current = true;
      if (animClass) {
        const t = setTimeout(
          () => afterOpenChangeRef.current?.(true),
          ANIM_DURATION,
        );
        return () => clearTimeout(t);
      }
      return;
    }
    if (wasVisibleRef.current) {
      wasVisibleRef.current = false;
      afterOpenChangeRef.current?.(false);
    }
  }, [visible, animClass]);

  // Keyboard handler — uses global escape stack so only the topmost overlay closes
  useEffect(() => {
    if (!visible || !keyboard) return;
    const handler = () => onCancel?.();
    pushEscapeHandler(handler);
    return () => removeEscapeHandler(handler);
  }, [visible, keyboard, onCancel]);

  // Body scroll lock — skip when rendering inside a container
  useEffect(() => {
    if (visible && !resolvedContainer?.current) {
      const prev = document.body.style.overflow;
      document.body.style.overflow = "hidden";
      return () => {
        document.body.style.overflow = prev;
      };
    }
  }, [visible, resolvedContainer]);

  if (!visible && destroyOnClose) return null;
  if (!visible) return null;

  const config = SIZE_CONFIG[size] ?? SIZE_CONFIG.default;
  const resolvedWidth = widthProp ?? config.width;
  const resolvedDialogStyle =
    widthProp && size !== "default"
      ? { ...config.dialogStyle, maxWidth: widthProp }
      : config.dialogStyle;

  const isLoading = confirmLoading || okButtonProps?.loading;

  const defaultFooter = (
    <div className="flex justify-end gap-2 pt-4">
      <Button onClick={onCancel} disabled={cancelButtonProps?.disabled}>
        {cancelText}
      </Button>
      <Button
        variant={okButtonProps?.danger ? "danger" : "primary"}
        loading={isLoading}
        disabled={okButtonProps?.disabled}
        onClick={onOk}
      >
        {okText}
      </Button>
    </div>
  );

  const renderedFooter = footer === null ? null : (footer ?? defaultFooter);

  const isInline = !!resolvedContainer?.current;
  const portalTarget = resolvedContainer?.current ?? document.body;

  return createPortal(
    <div
      className={cn(
        isInline
          ? "absolute inset-0 flex justify-center transition-colors duration-200"
          : "fixed inset-0 flex justify-center transition-colors duration-200",
        isInline
          ? "items-start overflow-y-auto"
          : size === "inset" ||
              size === "form" ||
              size === "almost-full" ||
              centered
            ? "items-center overflow-hidden"
            : "items-start overflow-y-auto",
        animClass ? "bg-black/35 backdrop-blur-sm" : "bg-black/0",
        size === "full" && "items-stretch",
        wrapClassName,
      )}
      style={{ zIndex, ...THIN_SCROLLBAR }}
      role="presentation"
      onMouseDown={(e) => {
        mouseDownOnMask.current = e.target === e.currentTarget;
      }}
      onClick={(e) => {
        if (
          maskClosable &&
          e.target === e.currentTarget &&
          mouseDownOnMask.current
        ) {
          onCancel?.();
        }
        mouseDownOnMask.current = false;
      }}
    >
      {/* Dialog */}
      <div
        ref={contentRef}
        className={cn(
          "relative rounded-lg shadow-2xl flex flex-col shrink-0 transition-all duration-200",
          "bg-[var(--surface)] text-[var(--text-primary)] border border-[var(--border)] shadow-[0_8px_32px_rgba(0,0,0,0.3)]",
          animClass
            ? "opacity-100 scale-100 translate-y-0"
            : "opacity-0 scale-95 translate-y-4",
          size === "full" && "!rounded-none",
          isInline
            ? size !== "full" && "mt-[5%] mb-[5%]"
            : !centered &&
                size !== "full" &&
                size !== "almost-full" &&
                size !== "inset" &&
                size !== "form" &&
                "mt-[10vh] mb-[10vh]",
          className,
        )}
        style={{
          width: resolvedWidth,
          maxWidth:
            size === "default"
              ? isInline
                ? "calc(100% - 32px)"
                : "calc(100vw - 32px)"
              : undefined,
          ...resolvedDialogStyle,
          ...style,
        }}
        role="dialog"
        aria-modal="true"
        aria-labelledby={title ? titleId : undefined}
      >
        {/* Header */}
        {(title || closable) && (
          <div className="flex items-center justify-between px-6 py-3 border-b border-black/[0.06] dark:border-white/[0.08] shrink-0">
            {title ? (
              <h3
                id={titleId}
                className="text-base font-semibold text-[var(--text-primary)] m-0"
              >
                {title}
              </h3>
            ) : (
              <span />
            )}
            <div className="flex items-center gap-1">
              {extra}
              {closable ? (
                <button
                  type="button"
                  className="text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors cursor-pointer"
                  onClick={onCancel}
                  aria-label="Close"
                >
                  <X className="h-5 w-5" />
                </button>
              ) : null}
            </div>
          </div>
        )}
        {/* Body */}
        <div
          className="px-6 py-4"
          style={{
            ...config.bodyStyle,
            ...bodyStyle,
            ...styles?.body,
            ...(config.containerStyle ?? {}),
          }}
        >
          {renderedChildren}
        </div>
        {/* Footer */}
        {renderedFooter ? (
          <div className="px-6 pb-4 shrink-0">{renderedFooter}</div>
        ) : null}
      </div>
    </div>,
    portalTarget,
  );
}

/* ─── Modal.confirm utility ─── */
/**
 * Creates a temporary React root and renders a Modal.
 * Automatically renders inside the active FloatingWindow when available.
 */
Modal.confirm = (config: ConfirmConfig) => {
  // Capture the active window container at call time
  const containerRef = activeModalContainerRef;

  const container = document.createElement("div");
  document.body.appendChild(container);

  const root = createRoot(container);

  const destroy = () => {
    root.unmount();
    container.remove();
  };

  const ConfirmDialog = () => {
    const [open, setOpen] = useState(true);
    const [loading, setLoading] = useState(false);
    return (
      <ModalContainerContext value={containerRef}>
        <Modal
          open={open}
          title={config.title}
          okText={config.okText ?? "OK"}
          cancelText={config.cancelText ?? "Cancel"}
          okButtonProps={config.okButtonProps}
          confirmLoading={loading}
          maskClosable={!loading}
          onOk={async () => {
            setLoading(true);
            try {
              await config.onOk?.();
              setOpen(false);
              setTimeout(destroy, 300);
            } finally {
              setLoading(false);
            }
          }}
          onCancel={() => {
            if (loading) return;
            config.onCancel?.();
            setOpen(false);
            setTimeout(destroy, 300);
          }}
        >
          {config.content}
        </Modal>
      </ModalContainerContext>
    );
  };

  root.render(<ConfirmDialog />);
};


export type { ConfirmConfig, ModalProps, ScaledModalSize } from "./Modal.types";
