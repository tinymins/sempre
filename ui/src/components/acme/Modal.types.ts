import type { CSSProperties, ReactNode, RefObject } from "react";

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
