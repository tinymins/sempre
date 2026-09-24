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
  useInteractions,
  useRole,
} from "@floating-ui/react";
import { AlertCircle } from "lucide-react";
import {
  cloneElement,
  isValidElement,
  type ReactElement,
  type ReactNode,
  useState,
} from "react";
import { Button } from "./Button";
import { FloatingVibrancy } from "./FloatingVibrancy";
import { cn } from "./utils";

export interface PopconfirmProps {
  /** Confirm title */
  title: ReactNode;
  /** Description text */
  description?: ReactNode;
  /** Placement */
  placement?: Placement;
  /** OK text */
  okText?: string;
  /** Cancel text */
  cancelText?: string;
  /** OK button type */
  okType?: "primary" | "danger";
  /** Confirm callback */
  // biome-ignore lint/suspicious/noConfusingVoidType: void in union is intentional for callback compatibility
  onConfirm?: () => void | Promise<unknown>;
  /** Cancel callback */
  onCancel?: () => void;
  /** Trigger element */
  children: ReactElement;
  /** Custom icon */
  icon?: ReactNode;
  /** Open state */
  open?: boolean;
  /** Open change */
  onOpenChange?: (open: boolean) => void;
  /** Disabled */
  disabled?: boolean;
}

export function Popconfirm({
  title,
  description,
  placement = "top",
  okText = "确定",
  cancelText = "取消",
  okType = "primary",
  onConfirm,
  onCancel,
  children,
  icon,
  open: openProp,
  onOpenChange,
  disabled = false,
}: PopconfirmProps) {
  const [uncontrolledOpen, setUncontrolledOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const open = openProp ?? uncontrolledOpen;

  const setOpen = (v: boolean) => {
    if (openProp === undefined) setUncontrolledOpen(v);
    onOpenChange?.(v);
  };

  const { refs, floatingStyles, context } = useFloating({
    open,
    onOpenChange: setOpen,
    placement,
    middleware: [offset(8), flip(), shift({ padding: 5 })],
    whileElementsMounted: autoUpdate,
  });

  const click = useClick(context, { enabled: !disabled });
  const dismiss = useDismiss(context, { enabled: !loading });
  const role = useRole(context);

  const { getReferenceProps, getFloatingProps } = useInteractions([
    click,
    dismiss,
    role,
  ]);

  const handleConfirm = async () => {
    try {
      setLoading(true);
      await onConfirm?.();
      setOpen(false);
    } finally {
      setLoading(false);
    }
  };

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
              "z-[9999] rounded-lg overflow-hidden bg-white/90 dark:bg-[rgba(15,15,25,0.9)] backdrop-blur-xl border border-black/[0.06] dark:border-white/[0.08] shadow-lg p-4 max-w-xs",
            )}
            {...getFloatingProps()}
          >
            <FloatingVibrancy />
            <div className="relative flex items-start gap-2 mb-3">
              <span className="shrink-0 mt-0.5 text-amber-500 [&>svg]:w-4 [&>svg]:h-4">
                {icon ?? <AlertCircle className="h-4 w-4" />}
              </span>
              <div>
                <div className="text-sm font-medium text-[var(--text-primary)]">
                  {title}
                </div>
                {description ? (
                  <div className="mt-1 text-xs text-[var(--text-muted)]">
                    {description}
                  </div>
                ) : null}
              </div>
            </div>
            <div className="relative flex justify-end gap-2">
              <Button
                size="small"
                onClick={() => {
                  onCancel?.();
                  setOpen(false);
                }}
              >
                {cancelText}
              </Button>
              <Button
                size="small"
                variant={okType === "danger" ? "danger" : "primary"}
                loading={loading}
                onClick={handleConfirm}
              >
                {okText}
              </Button>
            </div>
          </div>
        </FloatingPortal>
      ) : null}
    </>
  );
}
