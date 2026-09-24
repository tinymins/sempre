import { type FormHTMLAttributes, type ReactNode, useEffect, useState } from "react";
import { FormContext } from "./FormContext";
import { FormItem } from "./FormItem";
import type { FieldValues, FormInstance } from "./Form.types";
import { useForm } from "./useForm";
import { cn } from "./utils";

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

Form.Item = FormItem;
Form.useForm = useForm;
Form.useWatch = useWatch;

export { FormItemTooltip } from "./FormItem";
export { useFormContext } from "./FormContext";
export { useForm } from "./useForm";
export type { FieldValues, FormInstance } from "./Form.types";
