import {
  cloneElement,
  createContext,
  type FormHTMLAttributes,
  type ReactNode,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";

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
// biome-ignore lint/suspicious/noExplicitAny: antd compat
export type FieldValues = Record<string, any>;
type FieldErrors = Record<string, string | undefined>;
type FieldRule = {
  required?: boolean;
  message?: string;
  type?: "url" | "email";
  min?: number;
  max?: number;
  pattern?: RegExp;
  // biome-ignore lint/suspicious/noExplicitAny: antd compat
  validator?: (rule: FieldRule, value: any) => Promise<void> | void;
  // biome-ignore lint/suspicious/noExplicitAny: antd compat
  [key: string]: any;
};

/** Rule can be a static object or a function returning a rule (antd compat) */
type FormRule = FieldRule | ((form: FormInstance) => FieldRule);

/* ─── Form Instance ─── */
export interface FormInstance<T extends FieldValues = FieldValues> {
  getFieldsValue: () => T;
  getFieldValue: (name: keyof T) => unknown;
  setFieldsValue: (values: Partial<T>) => void;
  setFieldValue: (name: keyof T, value: unknown) => void;
  resetFields: () => void;
  validateFields: () => Promise<T>;
  isFieldTouched: (name: keyof T) => boolean;
  /** Check if fields are touched (antd compat) */
  isFieldsTouched: (nameList?: (keyof T)[], allTouched?: boolean) => boolean;
  /** Internal — used by FormItem */
  _register: (
    name: string,
    rules?: FormRule[],
    onChange?: (v: unknown) => void,
  ) => void;
  _unregister: (name: string) => void;
  _getFieldError: (name: string) => string | undefined;
  _values: React.MutableRefObject<T>;
  _setValues: (v: T) => void;
  _errors: FieldErrors;
  _setErrors: React.Dispatch<React.SetStateAction<FieldErrors>>;
  _touched: Set<string>;
  _listeners: Map<string, (v: unknown) => void>;
  _rules: Map<string, FormRule[]>;
  _initialValues: T;
  _rerender: () => void;
  _watchers: Map<string, Set<(v: unknown) => void>>;
  _notifyWatchers: (name: string, value: unknown) => void;
  /** Internal — clear a single field's error on user input */
  _clearFieldError: (name: string) => void;
  /** Internal — onValuesChange callback set by Form component */
  _onValuesChange?: (changedField: string, allValues: T) => void;
}

export function useForm<T extends FieldValues = FieldValues>(
  initialValues?: Partial<T>,
): [FormInstance<T>] {
  const initRef = useRef<T>((initialValues ?? {}) as T);
  const valuesRef = useRef<T>({ ...initRef.current });
  const [, forceUpdate] = useState(0);
  const errorsRef = useRef<FieldErrors>({});
  const [errors, setErrors] = useState<FieldErrors>({});
  const touchedRef = useRef<Set<string>>(new Set());
  const listenersRef = useRef<Map<string, (v: unknown) => void>>(new Map());
  const rulesRef = useRef<Map<string, FormRule[]>>(new Map());
  const watchersRef = useRef<Map<string, Set<(v: unknown) => void>>>(new Map());

  const instance = useMemo<FormInstance<T>>(() => {
    const init = initRef.current;
    const _notifyWatchers = (name: string, value: unknown) => {
      const set = watchersRef.current.get(name);
      if (set) {
        for (const fn of set) fn(value);
      }
    };
    const inst: FormInstance<T> = {
      getFieldsValue: () => ({ ...valuesRef.current }),
      getFieldValue: (name) => valuesRef.current[name],
      setFieldsValue: (values) => {
        Object.assign(valuesRef.current, values);
        for (const [key, val] of Object.entries(values)) {
          listenersRef.current.get(key)?.(val);
          _notifyWatchers(key, val);
        }
        forceUpdate((n) => n + 1);
      },
      setFieldValue: (name, value) => {
        (valuesRef.current as Record<string, unknown>)[name as string] = value;
        listenersRef.current.get(name as string)?.(value);
        _notifyWatchers(name as string, value);
        forceUpdate((n) => n + 1);
      },
      resetFields: () => {
        valuesRef.current = { ...init };
        for (const [key, fn] of listenersRef.current) {
          fn((init as Record<string, unknown>)[key]);
        }
        for (const [key] of watchersRef.current) {
          _notifyWatchers(key, (init as Record<string, unknown>)[key]);
        }
        setErrors({});
        errorsRef.current = {};
        touchedRef.current.clear();
        forceUpdate((n) => n + 1);
      },
      validateFields: async () => {
        const newErrors: FieldErrors = {};
        for (const [name, rules] of rulesRef.current) {
          const value = valuesRef.current[name as keyof T];
          for (const rawRule of rules) {
            const rule =
              typeof rawRule === "function"
                ? rawRule(instance as unknown as FormInstance)
                : rawRule;
            const errMsg = await validateRule(rule, value, name);
            if (errMsg) {
              newErrors[name] = errMsg;
              break;
            }
          }
        }
        setErrors(newErrors);
        errorsRef.current = newErrors;
        const hasErrors = Object.values(newErrors).some(Boolean);
        if (hasErrors) {
          throw new Error("Validation failed");
        }
        return { ...valuesRef.current };
      },
      isFieldTouched: (name) => touchedRef.current.has(name as string),
      isFieldsTouched: (nameList, allTouched) => {
        const names = nameList
          ? nameList.map((n) => String(n))
          : Array.from(rulesRef.current.keys());
        if (allTouched) return names.every((n) => touchedRef.current.has(n));
        return names.some((n) => touchedRef.current.has(n));
      },
      _register: (name, rules, onChange) => {
        if (rules) rulesRef.current.set(name, rules);
        if (onChange) listenersRef.current.set(name, onChange);
      },
      _unregister: (name) => {
        rulesRef.current.delete(name);
        listenersRef.current.delete(name);
      },
      _getFieldError: (name) => errorsRef.current[name],
      _values: valuesRef,
      _setValues: (v) => {
        valuesRef.current = v;
        forceUpdate((n) => n + 1);
      },
      _errors: errorsRef.current,
      _setErrors: setErrors,
      _touched: touchedRef.current,
      _listeners: listenersRef.current,
      _rules: rulesRef.current,
      _initialValues: init,
      _rerender: () => forceUpdate((n) => n + 1),
      _clearFieldError: (name: string) => {
        if (errorsRef.current[name]) {
          const next = { ...errorsRef.current };
          delete next[name];
          errorsRef.current = next;
          setErrors(next);
        }
      },
      _watchers: watchersRef.current,
      _notifyWatchers: _notifyWatchers,
    };
    return inst;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Keep errors synced
  instance._errors = errors;

  return [instance];
}

async function validateRule(
  rule: FieldRule,
  value: unknown,
  name: string,
): Promise<string | undefined> {
  const strVal = typeof value === "string" ? value : "";

  if (rule.required) {
    if (
      value === undefined ||
      value === null ||
      value === "" ||
      (Array.isArray(value) && value.length === 0)
    ) {
      return rule.message ?? `${name} 为必填项`;
    }
  }

  if (rule.type === "url" && strVal) {
    try {
      new URL(strVal);
    } catch {
      return rule.message ?? "请输入有效的 URL";
    }
  }

  if (rule.type === "email" && strVal) {
    if (!/\S+@\S+\.\S+/.test(strVal)) {
      return rule.message ?? "请输入有效的邮箱";
    }
  }

  if (rule.min !== undefined && strVal && strVal.length < rule.min) {
    return rule.message ?? `至少 ${rule.min} 个字符`;
  }

  if (rule.max !== undefined && strVal && strVal.length > rule.max) {
    return rule.message ?? `最多 ${rule.max} 个字符`;
  }

  if (rule.pattern && strVal && !rule.pattern.test(strVal)) {
    return rule.message ?? "格式不正确";
  }

  if (rule.validator) {
    try {
      await rule.validator(rule, value);
    } catch (err) {
      return rule.message ?? (err instanceof Error ? err.message : "验证失败");
    }
  }
  return undefined;
}

/* ─── Form Context ─── */
const FormContext = createContext<FormInstance | null>(null);

export function useFormContext() {
  return useContext(FormContext);
}

/* ─── Form ─── */
export interface FormProps
  extends Omit<FormHTMLAttributes<HTMLFormElement>, "onSubmit"> {
  /** Form instance from useForm() */
  form?: FormInstance;
  /** Layout */
  layout?: "horizontal" | "vertical" | "inline";
  /** Label column span (for horizontal) */
  labelCol?: { span?: number };
  /** Wrapper column span (for horizontal) */
  wrapperCol?: { span?: number };
  /** Initial values */
  initialValues?: FieldValues;
  /** Submit handler */
  // biome-ignore lint/suspicious/noExplicitAny: antd compat
  onFinish?: (values: any) => void;
  /** Finish failed */
  onFinishFailed?: (errorInfo: {
    errorFields: Array<{ name: string; errors: string[] }>;
  }) => void;
  /** Required mark display */
  requiredMark?: boolean;
  /** Called when any field value changes */
  onValuesChange?: (changedField: string, allValues: FieldValues) => void;
  /** Form size */
  size?: "small" | "middle" | "large";
  children?: ReactNode;
}

export function Form({
  form: formProp,
  layout = "vertical",
  labelCol,
  wrapperCol,
  initialValues,
  onFinish,
  onFinishFailed,
  requiredMark: _requiredMark,
  onValuesChange,
  size: _size,
  className,
  children,
  ...rest
}: FormProps) {
  const [internalForm] = useForm(initialValues);
  const form = formProp ?? internalForm;

  // Set initial values on external form after mount to avoid state updates during render
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentional mount-only init
  useEffect(() => {
    if (initialValues && formProp) {
      const current = form.getFieldsValue();
      const needsInit = Object.keys(initialValues).some(
        (k) => current[k] === undefined,
      );
      if (needsInit) {
        form.setFieldsValue(initialValues);
      }
    }
  }, []);

  // Sync onValuesChange callback to form instance
  form._onValuesChange = onValuesChange;

  return (
    <FormContext.Provider value={form}>
      <form
        className={cn(
          layout === "inline" && "flex flex-wrap gap-4",
          layout === "vertical" && "space-y-5",
          className,
        )}
        onSubmit={async (e) => {
          e.preventDefault();
          try {
            const values = await form.validateFields();
            await onFinish?.(values);
          } catch {
            const errorFields = Object.entries(form._errors)
              .filter(([, v]) => v)
              .map(([k, v]) => ({
                name: k,
                errors: [v as string],
              }));
            onFinishFailed?.({ errorFields });
          }
        }}
        {...rest}
      >
        {children}
      </form>
    </FormContext.Provider>
  );
}

/* ─── Form.Item ─── */
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

Form.Item = function FormItem({
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
};

Form.useForm = useForm;

/** Watch a specific form field value reactively (antd compat) */
export function useWatch<T = unknown>(
  name: string,
  form: FormInstance,
): T | undefined {
  const [value, setValue] = useState<T | undefined>(
    () => form.getFieldValue(name) as T | undefined,
  );

  useEffect(() => {
    // Sync current value on mount
    setValue(form.getFieldValue(name) as T | undefined);

    const callback = (v: unknown) => setValue(v as T | undefined);
    let set = form._watchers.get(name);
    if (!set) {
      set = new Set();
      form._watchers.set(name, set);
    }
    set.add(callback);

    return () => {
      set.delete(callback);
      if (set.size === 0) form._watchers.delete(name);
    };
  }, [name, form]);

  return value;
}

Form.useWatch = useWatch;
