import { X } from "lucide-react";
import {
  type CSSProperties,
  createContext,
  type ReactNode,
  type RefObject,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { createRoot } from "react-dom/client";
import { Button } from "./Button";
import { pushEscapeHandler, removeEscapeHandler } from "./escape-stack";
import { cn } from "./utils";

/* ─── Modal container context ─── */
/** When set, Modal portals into this element with absolute positioning instead of fullscreen. */
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
export type ScaledModalSize =
  | "full"
  | "almost-full"
  | "large"
  | "default"
  | "inset"
  | "form";

export interface ModalProps {
  /** Whether visible */
  open?: boolean;
  /** Title */
  title?: ReactNode;
  /** OK text */
  okText?: string;
  /** Cancel text */
  cancelText?: string;
  /** OK button props */
  okButtonProps?: {
    danger?: boolean;
    loading?: boolean;
    disabled?: boolean;
  };
  /** Cancel button props */
  cancelButtonProps?: {
    disabled?: boolean;
  };
  /** OK callback */
  onOk?: () => undefined | Promise<unknown>;
  /** Cancel / close callback */
  onCancel?: () => void;
  /** Custom footer — pass null to hide */
  footer?: ReactNode | null;
  /** Width (number or CSS string) */
  width?: number | string;
  /** ScaledModal size mode */
  size?: ScaledModalSize;
  /** Close on mask click */
  maskClosable?: boolean;
  /** Keyboard closable (Esc) */
  keyboard?: boolean;
  /** Close icon visible */
  closable?: boolean;
  /** Destroy on close */
  destroyOnClose?: boolean;
  /** Alias for destroyOnClose (antd compat) */
  destroyOnHidden?: boolean;
  /** Confirm loading */
  confirmLoading?: boolean;
  /** Z-index */
  zIndex?: number;
  /** Body style */
  bodyStyle?: CSSProperties;
  /** Styles API (antd v5 compat) */
  styles?: { body?: CSSProperties; root?: CSSProperties; mask?: CSSProperties };
  /** Container style */
  style?: CSSProperties;
  /** CSS class for wrapper */
  className?: string;
  /** CSS class for body */
  wrapClassName?: string;
  children?: ReactNode;
  /** Extra content rendered to the left of the close button in the header */
  extra?: ReactNode;
  /** Centered vertically */
  centered?: boolean;
  /** After open animation callback */
  afterOpenChange?: (open: boolean) => void;
  /** Portal target — when provided the modal renders inside this element with absolute positioning instead of fullscreen */
  container?: RefObject<HTMLElement | null>;
}

const THIN_SCROLLBAR: CSSProperties = {
  scrollbarWidth: "thin",
  scrollbarColor: "rgba(128,128,128,0.4) transparent",
};

interface SizeConfig {
  width: string | number;
  dialogStyle: CSSProperties;
  bodyStyle: CSSProperties;
  containerStyle?: CSSProperties;
}

const SIZE_CONFIG: Record<ScaledModalSize, SizeConfig> = {
  full: {
    width: "100%",
    dialogStyle: {
      maxWidth: "100%",
      margin: 0,
      padding: 0,
      borderRadius: 0,
      height: "100%",
    },
    bodyStyle: {
      flex: 1,
      minHeight: 0,
      overflowY: "auto",
      overflowX: "hidden",
      ...THIN_SCROLLBAR,
    },
    containerStyle: {
      display: "flex",
      flexDirection: "column",
      height: "100%",
    },
  },
  "almost-full": {
    width: "calc(100% - 48px)",
    dialogStyle: {
      maxWidth: "calc(100% - 48px)",
      height: "calc(100% - 48px)",
    },
    bodyStyle: {
      flex: 1,
      minHeight: 0,
      overflowY: "auto",
      overflowX: "hidden",
      ...THIN_SCROLLBAR,
    },
    containerStyle: {
      display: "flex",
      flexDirection: "column",
      height: "100%",
    },
  },
  large: {
    width: "90%",
    dialogStyle: { maxWidth: 1400 },
    bodyStyle: {
      maxHeight: "calc(100% - 200px)",
      overflowY: "auto",
      overflowX: "hidden",
      ...THIN_SCROLLBAR,
    },
  },
  /** 5% margin on all sides — 90% × 90%, no outer scrollbar */
  inset: {
    width: "90%",
    dialogStyle: {
      maxWidth: "90%",
      height: "90%",
    },
    bodyStyle: {
      flex: 1,
      minHeight: 0,
      overflow: "hidden",
    },
    containerStyle: {
      display: "flex",
      flexDirection: "column",
      height: "100%",
    },
  },
  /** 15% margin top/bottom — 90% × at-most 70%, centered; left-right grid forms pass style={{ height: "70%" }} */
  form: {
    width: "90%",
    dialogStyle: {
      maxWidth: "90%",
      maxHeight: "70%",
    },
    bodyStyle: {
      flex: 1,
      minHeight: 0,
      overflowY: "auto",
      overflowX: "hidden",
      ...THIN_SCROLLBAR,
    },
    containerStyle: {
      display: "flex",
      flexDirection: "column",
      height: "100%",
    },
  },
  default: {
    width: 520,
    dialogStyle: {},
    bodyStyle: {},
  },
};

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
  afterOpenChangeRef.current = afterOpenChange;
  useEffect(() => {
    if (visible && animClass) {
      const t = setTimeout(
        () => afterOpenChangeRef.current?.(true),
        ANIM_DURATION,
      );
      return () => clearTimeout(t);
    }
    if (!visible) {
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
          "bg-white/72 dark:bg-[rgba(20,20,35,0.65)] backdrop-blur-2xl border border-white/[0.12] dark:border-white/[0.12] shadow-[0_8px_32px_rgba(0,0,0,0.3)]",
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
        role="presentation"
      >
        {/* Header */}
        {(title || closable) && (
          <div className="flex items-center justify-between px-6 py-3 border-b border-black/[0.06] dark:border-white/[0.08] shrink-0">
            {title ? (
              <h3 className="text-base font-semibold text-[var(--text-primary)] m-0">
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
export interface ConfirmConfig {
  title: ReactNode;
  content?: ReactNode;
  okText?: string;
  cancelText?: string;
  onOk?: () => undefined | Promise<unknown>;
  onCancel?: () => void;
  okButtonProps?: ModalProps["okButtonProps"];
  type?: "confirm" | "info" | "success" | "error" | "warning";
  /** Icon for confirm dialog */
  icon?: ReactNode;
  /** OK button type (antd compat) */
  okType?: string;
}

/**
 * Imperative confirm dialog.
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

/**
 * Hook-based confirm dialog that inherits ModalContainerContext.
 * Unlike Modal.confirm(), the dialog renders inside the current React tree
 * so it appears within FloatingWindow when used in a window context.
 *
 * Usage:
 *   const [confirmHolder, confirm] = useConfirm();
 *   confirm({ title: "Delete?", onOk: () => ... });
 *   return <>{confirmHolder}<div>...</div></>;
 */
export function useConfirm(): [ReactNode, (config: ConfirmConfig) => void] {
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const configRef = useRef<ConfirmConfig | null>(null);
  const loadingRef = useRef(false);
  const [, forceUpdate] = useState(0);

  const confirm = useCallback((config: ConfirmConfig) => {
    configRef.current = config;
    loadingRef.current = false;
    setLoading(false);
    setOpen(true);
    forceUpdate((n) => n + 1);
  }, []);

  const contextHolder = (
    <Modal
      open={open}
      title={configRef.current?.title}
      okText={configRef.current?.okText ?? "确定"}
      cancelText={configRef.current?.cancelText ?? "取消"}
      okButtonProps={configRef.current?.okButtonProps}
      confirmLoading={loading}
      maskClosable={!loading}
      onOk={async () => {
        loadingRef.current = true;
        setLoading(true);
        try {
          await configRef.current?.onOk?.();
          setOpen(false);
        } finally {
          loadingRef.current = false;
          setLoading(false);
        }
      }}
      onCancel={() => {
        if (loadingRef.current) return;
        configRef.current?.onCancel?.();
        setOpen(false);
      }}
    >
      {configRef.current?.content}
    </Modal>
  );

  return [contextHolder, confirm];
}
