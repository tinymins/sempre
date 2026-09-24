import { cloneElement, type ReactNode, useEffect, useId, useMemo, useRef, useState } from "react";
import { useFormContext } from "./FormContext";
import type { FieldValues, FormInstance, FormRule } from "./Form.types";
import { QuestionCircleOutlined } from "./icons";
import { Tooltip } from "./Tooltip";
import { cn } from "./utils";

/* ─── FormItemTooltip ─── */
/**
 * 表单标签旁的 ? 问号图标，悬停显示提示内容。
 * 颜色使用 blue-400，与下方帮助文本（text-muted）区分。
 */
export function FormItemTooltip({ content }: { content: ReactNode }) {
  return (
    <Tooltip title={content} placement="right">
      <span className="inline-flex cursor-help">
        <QuestionCircleOutlined
          size={13}
          className="text-[var(--accent-muted)] hover:text-[var(--accent)] transition-colors"
        />
      </span>
    </Tooltip>
  );
}

/* ─── Types ─── */

export interface FormItemProps {
  /** Field name */
  name?: string;
  /** Label */
  label?: ReactNode;
  /** Validation rules */
  rules?: FormRule[];
  /** Required shorthand */
  required?: boolean;
  /** Extra info below input */
  extra?: ReactNode;
  /** The value prop name (default: "value") */
  valuePropName?: string;
  /** Trigger event name (default: "onChange") */
  trigger?: string;
  /** Initial value for this field */
  initialValue?: unknown;
  /** Whether to hide the field */
  hidden?: boolean;
  /** Conditional field rendering via shouldUpdate */
  shouldUpdate?:
    | boolean
    | ((prevValues: FieldValues, curValues: FieldValues) => boolean);
  /** Tooltip for label */
  tooltip?: ReactNode;
  /** No style wrapper (antd compat) */
  noStyle?: boolean;
  /** Field dependencies */
  dependencies?: string[];
  /** Layout direction */
  layout?: "horizontal" | "vertical";
  /** Label col span */
  labelCol?: { span?: number };
  /** Wrapper col span */
  wrapperCol?: { span?: number };
  className?: string;
  style?: React.CSSProperties;
  children?: ReactNode | ((form: FormInstance) => ReactNode);
}

export function FormItem({
  name,
  label,
  rules = [],
  required,
  extra,
  valuePropName = "value",
  trigger = "onChange",
  initialValue,
  hidden = false,
  shouldUpdate: _shouldUpdate,
  noStyle = false,
  tooltip,
  className,
  style,
  children,
}: FormItemProps) {
  const form = useFormContext();
  const [, rerender] = useState(0);
  const generatedId = useId();
  const controlWrapperRef = useRef<HTMLDivElement>(null);

  // Merge required into rules if not already
  const mergedRules = useMemo(() => {
    if (required && !rules.some((r) => typeof r !== "function" && r.required)) {
      return [
        { required: true, message: `${label ?? name} 为必填项` },
        ...rules,
      ];
    }
    return rules;
  }, [rules, required, label, name]);

  // Register field
  const onChangeRef = useRef<((v: unknown) => void) | undefined>(undefined);
  onChangeRef.current = (_v: unknown) => rerender((n) => n + 1);

  useMemo(() => {
    if (name && form) {
      form._register(name, mergedRules, onChangeRef.current);
      // Set initial value if provided
      if (
        initialValue !== undefined &&
        form._values.current[name] === undefined
      ) {
        (form._values.current as Record<string, unknown>)[name] = initialValue;
      }
    }
  }, [name, form, mergedRules, initialValue]);

  // Unregister when field unmounts (e.g. type switching removes old fields)
  useEffect(() => {
    return () => {
      if (name && form) {
        form._unregister(name);
      }
    };
  }, [name, form]);

  if (hidden) return null;

  // shouldUpdate + render function support
  if (typeof children === "function" && form) {
    return <>{(children as (form: FormInstance) => ReactNode)(form)}</>;
  }

  const error = name ? form?._errors[name] : undefined;
  const value = name
    ? (form?._values.current as Record<string, unknown>)?.[name]
    : undefined;

  // Clone child with value and onChange
  let child = children;
  let fieldId = name ?? generatedId;
  if (
    name &&
    form &&
    children &&
    typeof children !== "string" &&
    typeof children !== "number"
  ) {
    const childEl = children as React.ReactElement;
    if (childEl && typeof childEl === "object" && "type" in childEl) {
      const childProps = childEl.props as { id?: string };
      fieldId = childProps.id ?? name;
      const injectedProps: Record<string, unknown> = {
        [valuePropName]:
          value !== undefined
            ? value
            : valuePropName === "checked"
              ? false
              : undefined,
        [trigger]: (...args: unknown[]) => {
          let newValue: unknown;
          // Handle native events
          if (
            args[0] &&
            typeof args[0] === "object" &&
            "target" in (args[0] as Record<string, unknown>)
          ) {
            const target = (args[0] as React.ChangeEvent<HTMLInputElement>)
              .target;
            newValue =
              target.type === "checkbox" ? target.checked : target.value;
          } else {
            newValue = args[0];
          }
          (form._values.current as Record<string, unknown>)[name] = newValue;
          form._touched.add(name);
          form._clearFieldError(name);
          form._notifyWatchers(name, newValue);
          form._onValuesChange?.(name, { ...form._values.current });
          form._rerender();
          // Call original handler
          const originalHandler = (childEl.props as Record<string, unknown>)[
            trigger
          ];
          if (typeof originalHandler === "function") {
            originalHandler(...args);
          }
        },
        id: fieldId,
        name,
      };
      if (error) {
        injectedProps.status = "error";
      }

      child = cloneElement(childEl, injectedProps);
    }
  }

  const focusFieldControl = () => {
    const target =
      (typeof document !== "undefined"
        ? document.getElementById(fieldId)
        : null) ??
      controlWrapperRef.current?.querySelector<HTMLElement>(
        'input:not([type="hidden"]):not([disabled]), textarea:not([disabled]), select:not([disabled]), button:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ) ??
      null;
    target?.focus();
  };

  // noStyle: render child directly without wrapper divs
  if (noStyle) {
    return <>{child as ReactNode}</>;
  }

  return (
    <div className={cn("w-full", className)} style={style}>
      {label ? (
        <div className="mb-2 flex items-center gap-1.5">
          <label
            htmlFor={fieldId}
            className="text-sm font-medium text-[var(--text-primary)]"
            onMouseDown={() => {
              focusFieldControl();
            }}
          >
            {required ||
            mergedRules.some((r) => typeof r !== "function" && r.required) ? (
              <span className="text-red-500 mr-0.5">*</span>
            ) : null}
            {label}
          </label>
          {tooltip ? <FormItemTooltip content={tooltip} /> : null}
        </div>
      ) : null}
      <div ref={controlWrapperRef} className="[&>:not(button)]:w-full">
        {child as ReactNode}
      </div>
      {error ? <div className="mt-1 text-xs text-red-500">{error}</div> : null}
      {extra ? (
        <div className="mt-1 text-xs text-[var(--text-muted)]">{extra}</div>
      ) : null}
    </div>
  );
}
