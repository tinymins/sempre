import { useMemo, useRef, useState } from "react";
import type { FieldErrors, FieldRule, FieldValues, FormInstance, FormRule } from "./Form.types";

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
