import type { CSSProperties, ReactNode } from "react";

export interface SelectOption {
  label: ReactNode;
  value: string | number;
  disabled?: boolean;
  /** Label shown in selected tags. Falls back to `label` if not set. */
  tagLabel?: ReactNode;
}

export interface SelectProps {
  /** Options */
  options?: SelectOption[];
  /** Value (controlled) */
  value?: string | number | (string | number)[];
  /** Default value */
  defaultValue?: string | number | (string | number)[];
  /** Change handler */
  // biome-ignore lint/suspicious/noExplicitAny: antd compat
  onChange?: (value: any, option?: any) => void;
  /** Placeholder */
  placeholder?: string;
  /** Allow clear */
  allowClear?: boolean;
  /** Multiple select */
  mode?: "multiple" | "tags";
  /** Disabled */
  disabled?: boolean;
  /** Size */
  size?: "small" | "middle" | "large";
  /** Status */
  status?: "error" | "warning";
  /** Loading */
  loading?: boolean;
  /** Show search */
  showSearch?: boolean;
  /** Filter option */
  filterOption?: boolean | ((input: string, option?: SelectOption) => boolean);
  /** Not found content */
  notFoundContent?: ReactNode;
  /** Style */
  style?: CSSProperties;
  className?: string;
  /** Dropdown class */
  popupClassName?: string;
  /** Match dropdown width to the select trigger */
  popupMatchSelectWidth?: boolean;
  /** Option label prop (compatibility) */
  optionFilterProp?: string;
  /** Field names customization */
  fieldNames?: { label?: string; value?: string };
  /** Enable virtual scrolling for large option lists */
  virtual?: boolean;
  /** Children (Select.Option pattern) */
  children?: ReactNode;
}
