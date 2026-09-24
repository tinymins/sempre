import type { ReactNode } from "react";
import type { PaginationProps } from "./Pagination";

export interface TableColumn<T = Record<string, unknown>> {
  /** Column title */
  title?: ReactNode;
  /** Data key (string for flat, string[] for nested paths) */
  dataIndex?: string | string[];
  /** Unique key */
  key?: string;
  /** Render function */
  // biome-ignore lint/suspicious/noExplicitAny: antd compat
  render?: (value: any, record: T, index: number) => ReactNode;
  /** Sorter */
  sorter?: boolean | ((a: T, b: T) => number);
  /** Column filters */
  filters?: { text: ReactNode; value: string }[];
  /** Width */
  width?: number | string;
  /** Min width */
  minWidth?: number | string;
  /** Alignment */
  align?: "left" | "center" | "right";
  /** Fixed column */
  fixed?: "left" | "right";
  /** Ellipsis overflow */
  ellipsis?: boolean;
  /** Custom class for cell */
  className?: string;
  /** Column children for grouping */
  children?: TableColumn<T>[];
}

export interface TableProps<T = Record<string, unknown>> {
  /** Column definitions */
  columns?: TableColumn<T>[];
  /** Data source */
  dataSource?: T[];
  /** Row key */
  rowKey?: string | ((record: T, index: number) => string);
  /** Loading state */
  loading?: boolean;
  /** Bordered */
  bordered?: boolean;
  /** Size */
  size?: "small" | "middle" | "large";
  /** Custom empty content */
  locale?: { emptyText?: ReactNode };
  /** Pagination config (false to hide) */
  pagination?: false | PaginationProps;
  /** Change callback */
  onChange?: (pagination: { current: number; pageSize: number }) => void;
  /** Scroll config */
  scroll?: { x?: number | string; y?: number | string };
  /** Expand config for tree data */
  expandable?: {
    defaultExpandAllRows?: boolean;
    expandedRowKeys?: string[];
    onExpand?: (expanded: boolean, record: T) => void;
    childrenColumnName?: string;
    indentSize?: number;
    /** Render expanded row content */
    expandedRowRender?: (
      record: T,
      index: number,
      expanded: boolean,
    ) => ReactNode;
    /** Control which rows can expand (default: all if expandedRowRender is set) */
    rowExpandable?: (record: T) => boolean;
  };
  /** Row class name */
  rowClassName?: string | ((record: T, index: number) => string);
  /** On row handler */
  onRow?: (
    record: T,
    index: number,
  ) => React.HTMLAttributes<HTMLTableRowElement>;
  /** Title above table */
  title?: () => ReactNode;
  /** Summary below table */
  summary?: () => ReactNode;
  /** Extra class */
  className?: string;
  /** Style */
  style?: React.CSSProperties;
  /** Default expand all rows (shorthand for expandable.defaultExpandAllRows) */
  defaultExpandAllRows?: boolean;
  /** Row selection config */
  rowSelection?: {
    /** Currently selected row keys */
    selectedRowKeys?: React.Key[];
    /** Change handler */
    onChange?: (selectedRowKeys: React.Key[], selectedRows: T[]) => void;
    /** Selection type */
    type?: "checkbox" | "radio";
    /** Row selectable predicate */
    getCheckboxProps?: (record: T) => { disabled?: boolean };
  };
  /** Virtual scroll — only renders visible rows. Requires scroll.y to be set. */
  virtual?: boolean;
  /** Row height (px) estimate for virtual scroll (defaults: small=33, middle=41, large=49). */
  itemHeight?: number;
  /** Called with reordered array after drag-sort. Enables a drag-handle column. */
  onReorder?: (reordered: T[]) => void;
  /** Disable drag sorting (e.g. during a pending mutation) */
  sortDisabled?: boolean;
}

/** Resolve a stable string key from a column's key/dataIndex. */
