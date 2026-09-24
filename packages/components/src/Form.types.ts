export type FieldValues = Record<string, any>;
export type FieldErrors = Record<string, string | undefined>;
export type FieldRule = {
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
export type FormRule = FieldRule | ((form: FormInstance) => FieldRule);

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
