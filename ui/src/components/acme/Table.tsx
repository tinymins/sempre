import { ChevronDown, ChevronsUpDown, ChevronUp, Loader2 } from "lucide-react";
import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Checkbox } from "./Checkbox";
import { DragHandle, useDnd } from "./dnd";
import { Empty } from "./Empty";
import { Pagination, type PaginationProps } from "./Pagination";
import { cn } from "./utils";

/* ─── Types ─── */
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
function colKey(
  col: { key?: string; dataIndex?: string | string[] },
  fallback: number | string,
): string | number {
  if (col.key) return col.key;
  if (col.dataIndex)
    return Array.isArray(col.dataIndex)
      ? col.dataIndex.join(".")
      : col.dataIndex;
  return fallback;
}

/** Get value from a record by dot path or array path */
function getNestedValue(
  obj: Record<string, unknown>,
  path?: string | string[],
): unknown {
  if (!path) return undefined;
  const keys = Array.isArray(path) ? path : path.split(".");
  return keys.reduce<unknown>((o, k) => {
    if (o && typeof o === "object") return (o as Record<string, unknown>)[k];
    return undefined;
  }, obj);
}

function getKey<T>(
  record: T,
  rowKey: string | ((r: T, index: number) => string),
  idx: number,
): string {
  if (typeof rowKey === "function") return rowKey(record, idx);
  const val = (record as Record<string, unknown>)[rowKey];
  return val != null ? String(val) : String(idx);
}

/* ─── Tree Row Renderer ─── */
function renderRows<T>(
  dataSource: T[],
  columns: TableColumn<T>[],
  rowKey: string | ((r: T, index: number) => string),
  expandable: TableProps<T>["expandable"],
  expandedKeys: Set<string>,
  toggleExpand: (key: string, record: T, expanded: boolean) => void,
  level: number,
  sizeClass: string,
  bordered: boolean,
  rowClassName?: TableProps<T>["rowClassName"],
  onRow?: TableProps<T>["onRow"],
  startIdx = { v: 0 },
): ReactNode[] {
  const childrenField = expandable?.childrenColumnName ?? "children";
  const indentSize = expandable?.indentSize ?? 20;
  const hasExpandRender = !!expandable?.expandedRowRender;
  const rows: ReactNode[] = [];

  for (const record of dataSource) {
    const idx = startIdx.v++;
    const key = getKey(record, rowKey, idx);
    const kids = (record as Record<string, unknown>)[childrenField] as
      | T[]
      | undefined;
    const hasKids = Array.isArray(kids) && kids.length > 0;
    const expanded = expandedKeys.has(key);
    const canExpand = hasExpandRender
      ? (expandable?.rowExpandable?.(record) ?? true)
      : hasKids;
    const rowCls =
      typeof rowClassName === "function"
        ? rowClassName(record, idx)
        : rowClassName;
    const rowProps = onRow?.(record, idx) ?? {};

    const { className: rowPropClassName, ...restRowProps } = rowProps;
    rows.push(
      <tr
        key={key}
        className={cn(
          "border-b border-black/[0.04] dark:border-white/[0.04] hover:bg-black/[0.02] dark:hover:bg-white/[0.03] transition-colors",
          rowCls,
          rowPropClassName,
        )}
        {...restRowProps}
      >
        {hasExpandRender && (
          <td
            className={cn(
              sizeClass,
              "text-center w-10",
              bordered &&
                "border-r border-black/[0.06] dark:border-white/[0.08]",
            )}
          >
            {canExpand && (
              <button
                type="button"
                className="cursor-pointer text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
                onClick={() => toggleExpand(key, record, !expanded)}
              >
                {expanded ? "−" : "+"}
              </button>
            )}
          </td>
        )}
        {columns.map((col, ci) => {
          const dataIndex = col.dataIndex ?? col.key;
          const value = getNestedValue(
            record as Record<string, unknown>,
            dataIndex,
          );
          const rendered = col.render
            ? col.render(value, record, idx)
            : (value as ReactNode);

          return (
            <td
              key={colKey(col, ci)}
              className={cn(
                sizeClass,
                col.align === "center" && "text-center",
                col.align === "right" && "text-right",
                col.ellipsis && "truncate max-w-0",
                bordered &&
                  "border-r border-black/[0.06] dark:border-white/[0.08] last:border-r-0",
                col.fixed === "left" && "sticky left-0 z-10",
                col.fixed === "right" && "sticky right-0 z-10",
                col.className,
              )}
              style={{ width: col.width, minWidth: col.minWidth }}
            >
              <span
                className="flex items-center gap-1 w-full"
                style={{
                  paddingLeft:
                    ci === 0 && !hasExpandRender
                      ? level * indentSize
                      : undefined,
                  justifyContent:
                    col.align === "center"
                      ? "center"
                      : col.align === "right"
                        ? "flex-end"
                        : undefined,
                }}
              >
                {ci === 0 && !hasExpandRender && hasKids ? (
                  <button
                    type="button"
                    className="shrink-0 text-[var(--text-muted)] hover:text-[var(--text-secondary)] w-4"
                    onClick={() => toggleExpand(key, record, !expanded)}
                  >
                    {expanded ? "▾" : "▸"}
                  </button>
                ) : ci === 0 && !hasExpandRender && level > 0 ? (
                  <span className="w-4 shrink-0" />
                ) : null}
                <span
                  className={cn(
                    "min-w-0",
                    !col.align || col.align === "left" ? "flex-1" : undefined,
                    col.ellipsis && "truncate",
                  )}
                >
                  {rendered}
                </span>
              </span>
            </td>
          );
        })}
      </tr>,
    );

    // Expanded row content (expandedRowRender)
    if (hasExpandRender && expanded && canExpand) {
      rows.push(
        <tr
          key={`${key}-expand`}
          className="bg-black/[0.015] dark:bg-white/[0.015]"
        >
          <td
            colSpan={columns.length + 1}
            className={cn(
              "px-4 py-2",
              bordered &&
                "border-b border-black/[0.06] dark:border-white/[0.08]",
            )}
          >
            {expandable.expandedRowRender?.(record, idx, expanded)}
          </td>
        </tr>,
      );
    }

    // Tree children
    if (!hasExpandRender && hasKids && expanded) {
      rows.push(
        ...renderRows(
          kids ?? [],
          columns,
          rowKey,
          expandable,
          expandedKeys,
          toggleExpand,
          level + 1,
          sizeClass,
          bordered,
          rowClassName,
          onRow,
          startIdx,
        ),
      );
    }
  }

  return rows;
}

/* ─── Table Component ─── */
export function Table<T = Record<string, unknown>>({
  columns = [],
  dataSource = [],
  rowKey = "id",
  loading = false,
  bordered = false,
  size = "middle",
  locale,
  pagination,
  onChange,
  scroll,
  expandable,
  defaultExpandAllRows,
  rowClassName,
  onRow,
  title,
  summary,
  className,
  style,
  rowSelection,
  virtual = false,
  itemHeight,
  onReorder,
  sortDisabled,
}: TableProps<T>) {
  // Expand state
  const [expandedKeysState, setExpandedKeysState] = useState<Set<string>>(
    () => {
      if (expandable?.defaultExpandAllRows || defaultExpandAllRows) {
        const keys = new Set<string>();
        const collect = (items: T[], childField: string) => {
          for (let i = 0; i < items.length; i++) {
            const key = getKey(items[i], rowKey, i);
            keys.add(key);
            const kids = (items[i] as Record<string, unknown>)[childField] as
              | T[]
              | undefined;
            if (Array.isArray(kids)) collect(kids, childField);
          }
        };
        collect(dataSource, expandable?.childrenColumnName ?? "children");
        return keys;
      }
      return new Set(expandable?.expandedRowKeys ?? []);
    },
  );

  const expandedKeys = expandable?.expandedRowKeys
    ? new Set(expandable.expandedRowKeys)
    : expandedKeysState;

  const toggleExpand = (key: string, record: T, expanded: boolean) => {
    const next = new Set(expandedKeys);
    if (expanded) next.add(key);
    else next.delete(key);
    setExpandedKeysState(next);
    expandable?.onExpand?.(expanded, record);
  };

  const sizeClass = {
    small: "px-2 py-1 text-xs",
    middle: "px-3 py-2 text-sm",
    large: "px-4 py-3 text-base",
  }[size];

  // Sort state
  const [sortState, setSortState] = useState<{
    key: string;
    dir: "asc" | "desc";
    fn: (a: T, b: T) => number;
  } | null>(null);

  const handleSortClick = (key: string, fn: (a: T, b: T) => number) => {
    setSortState((prev) => {
      if (prev?.key === key) {
        if (prev.dir === "asc") return { key, dir: "desc", fn };
        return null;
      }
      return { key, dir: "asc", fn };
    });
  };

  const sortedData = useMemo(() => {
    if (!sortState) return dataSource;
    const arr = [...dataSource].sort(sortState.fn);
    return sortState.dir === "desc" ? arr.reverse() : arr;
  }, [dataSource, sortState]);

  // Client-side pagination
  const paginationConfig = typeof pagination === "object" ? pagination : null;
  const [paginationState, setPaginationState] = useState({
    current: 1,
    pageSize: paginationConfig?.defaultPageSize ?? 10,
  });
  const paginatedData = useMemo(() => {
    if (!paginationConfig) return sortedData;
    const { current, pageSize } = paginationState;
    const start = (current - 1) * pageSize;
    return sortedData.slice(start, start + pageSize);
  }, [sortedData, paginationConfig, paginationState]);

  // Virtual scroll
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [containerH, setContainerH] = useState(0);
  const ROW_HEIGHT_MAP = { small: 33, middle: 41, large: 49 } as const;
  const rowHeight = itemHeight ?? ROW_HEIGHT_MAP[size];
  const OVERSCAN = 5;

  // Measure container height after mount and on resize
  useEffect(() => {
    const el = scrollContainerRef.current;
    if (!el || !virtual) return;
    const update = () => setContainerH(el.clientHeight);
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, [virtual]);

  const handleVirtualScroll = useCallback(
    (e: React.UIEvent<HTMLDivElement>) => {
      setScrollTop(e.currentTarget.scrollTop);
    },
    [],
  );

  const effectiveData = paginationConfig ? paginatedData : sortedData;
  const effectiveContainerH = containerH || 600;
  const visibleCount = Math.ceil(effectiveContainerH / rowHeight);
  const startIdx = virtual
    ? Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN)
    : 0;
  const endIdx = virtual
    ? Math.min(effectiveData.length - 1, startIdx + visibleCount + OVERSCAN * 2)
    : effectiveData.length - 1;
  const virtualTopH = startIdx * rowHeight;
  const virtualBottomH = Math.max(
    0,
    (effectiveData.length - endIdx - 1) * rowHeight,
  );
  const renderData = virtual
    ? effectiveData.slice(startIdx, endIdx + 1)
    : effectiveData;

  // Row selection helpers
  const selectedSet = new Set(rowSelection?.selectedRowKeys ?? []);
  const allKeys = dataSource.map((r, i) => getKey(r, rowKey, i));
  const allSelectableKeys = rowSelection?.getCheckboxProps
    ? allKeys.filter(
        (_, i) => !rowSelection.getCheckboxProps?.(dataSource[i]).disabled,
      )
    : allKeys;
  const allSelected =
    allSelectableKeys.length > 0 &&
    allSelectableKeys.every((k) => selectedSet.has(k));
  const someSelected = allSelectableKeys.some((k) => selectedSet.has(k));

  const toggleRow = (key: string, _record: T) => {
    if (!rowSelection?.onChange) return;
    const next = new Set(selectedSet);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    const nextKeys = Array.from(next);
    const nextRows = dataSource.filter((r, i) =>
      next.has(getKey(r, rowKey, i)),
    );
    rowSelection.onChange(nextKeys, nextRows);
  };

  const toggleAll = () => {
    if (!rowSelection?.onChange) return;
    if (allSelected) {
      // Deselect all
      const remaining = Array.from(selectedSet).filter(
        (k) => !allSelectableKeys.includes(String(k)),
      );
      const remainingRows = dataSource.filter((r, i) =>
        remaining.includes(getKey(r, rowKey, i)),
      );
      rowSelection.onChange(remaining, remainingRows);
    } else {
      // Select all
      const next = new Set([...selectedSet, ...allSelectableKeys]);
      const nextKeys = Array.from(next);
      const nextRows = dataSource.filter((r, i) =>
        next.has(getKey(r, rowKey, i)),
      );
      rowSelection.onChange(nextKeys, nextRows);
    }
  };

  // ── Drag-to-reorder (via useDnd) ──
  const dnd = useDnd({
    count: dataSource.length,
    onReorder: onReorder
      ? (from, to) => {
          const arr = [...dataSource];
          const [moved] = arr.splice(from, 1);
          arr.splice(to, 0, moved);
          onReorder(arr);
        }
      : undefined,
    disabled: sortDisabled || !onReorder,
  });

  const dndMergedOnRow = onReorder
    ? (record: T, index: number) => {
        const userProps = onRow?.(record, index) ?? {};
        return {
          ...userProps,
          ...dnd.getItemProps(index),
          style: { ...userProps.style, ...dnd.getItemStyle(index) },
        };
      }
    : onRow;

  // Effective columns (prepend drag + selection columns as needed)
  const dragColumn: TableColumn<T> | null = onReorder
    ? {
        key: "__dnd_drag__",
        title: "",
        width: 40,
        render: (_: unknown, __: T, idx: number) => (
          <DragHandle
            disabled={sortDisabled || dnd.isPending}
            isDragging={dnd.isDragging}
            {...dnd.getHandleProps(idx)}
          />
        ),
      }
    : null;

  const effectiveColumns: TableColumn<T>[] = [
    ...(dragColumn ? [dragColumn] : []),
    ...(rowSelection
      ? [
          {
            key: "__selection__" as const,
            title: (
              <Checkbox
                checked={allSelected}
                indeterminate={someSelected && !allSelected}
                onChange={toggleAll}
              />
            ) as unknown as string,
            width: 40,
            align: "center" as const,
            // biome-ignore lint/suspicious/noExplicitAny: antd table render signature compat
            render: (_: any, record: T, idx: number) => {
              const key = getKey(record, rowKey, idx);
              const cbProps = rowSelection.getCheckboxProps?.(record);
              return (
                <Checkbox
                  checked={selectedSet.has(key)}
                  disabled={cbProps?.disabled}
                  onChange={() => toggleRow(key, record)}
                />
              );
            },
          },
        ]
      : []),
    ...columns,
  ];

  return (
    <div className={cn("w-full", className)} style={style}>
      {title ? <div className="mb-2">{title()}</div> : null}
      <div
        ref={scrollContainerRef}
        className={cn(
          "overflow-auto rounded-lg border border-black/[0.06] dark:border-white/[0.08]",
          "backdrop-blur bg-white/[0.03] dark:bg-white/[0.02]",
        )}
        style={{
          ...(virtual && scroll?.y
            ? { height: scroll.y, overflowY: "scroll" as const }
            : { maxHeight: scroll?.y }),
          scrollbarWidth: "thin",
          scrollbarColor: "rgba(128,128,128,0.4) transparent",
        }}
        onScroll={virtual ? handleVirtualScroll : undefined}
      >
        <table
          className="w-full border-collapse"
          style={{ minWidth: scroll?.x }}
        >
          <thead>
            <tr className="bg-black/[0.02] dark:bg-white/[0.04]">
              {expandable?.expandedRowRender && (
                <th
                  className={cn(
                    sizeClass,
                    "text-center font-medium text-[var(--text-secondary)] whitespace-nowrap border-b border-black/[0.06] dark:border-white/[0.08] w-10",
                    bordered &&
                      "border-r border-black/[0.06] dark:border-white/[0.08]",
                  )}
                />
              )}
              {effectiveColumns.map((col, ci) => {
                const sortKey = String(colKey(col, ci));
                const isSortable = typeof col.sorter === "function";
                const isActiveSorted = isSortable && sortState?.key === sortKey;
                return (
                  <th
                    key={colKey(col, ci)}
                    className={cn(
                      sizeClass,
                      "text-left font-medium text-[var(--text-secondary)] whitespace-nowrap border-b border-black/[0.06] dark:border-white/[0.08]",
                      isSortable &&
                        "cursor-pointer select-none hover:text-[var(--text-primary)] transition-colors",
                      isActiveSorted && "!text-[var(--accent)]",
                      col.align === "center" && "text-center",
                      col.align === "right" && "text-right",
                      virtual &&
                        "sticky top-0 z-[1] bg-[rgba(252,252,255,0.96)] dark:bg-[rgba(14,14,24,0.96)]",
                      !virtual && col.fixed === "left" && "sticky left-0 z-10",
                      !virtual &&
                        col.fixed === "right" &&
                        "sticky right-0 z-10",
                      virtual && col.fixed === "left" && "left-0 z-[2]",
                      virtual && col.fixed === "right" && "right-0 z-[2]",
                      bordered &&
                        "border-r border-black/[0.06] dark:border-white/[0.08] last:border-r-0",
                    )}
                    onClick={
                      isSortable
                        ? () =>
                            handleSortClick(
                              sortKey,
                              col.sorter as (a: T, b: T) => number,
                            )
                        : undefined
                    }
                    style={{
                      width: col.width,
                      minWidth: col.minWidth,
                      textAlign: col.align ?? "left",
                    }}
                  >
                    <span
                      className="inline-flex items-center gap-1"
                      style={{
                        justifyContent:
                          col.align === "center"
                            ? "center"
                            : col.align === "right"
                              ? "flex-end"
                              : undefined,
                        width:
                          col.align && col.align !== "left"
                            ? "100%"
                            : undefined,
                      }}
                    >
                      {col.title}
                      {isSortable && (
                        <span
                          className={cn(
                            "inline-flex shrink-0",
                            isActiveSorted
                              ? "text-[var(--accent)]"
                              : "text-[var(--text-muted)] opacity-40",
                          )}
                        >
                          {isActiveSorted && sortState?.dir === "asc" ? (
                            <ChevronUp className="h-3.5 w-3.5" />
                          ) : isActiveSorted && sortState?.dir === "desc" ? (
                            <ChevronDown className="h-3.5 w-3.5" />
                          ) : (
                            <ChevronsUpDown className="h-3.5 w-3.5" />
                          )}
                        </span>
                      )}
                    </span>
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody className="bg-transparent">
            {loading ? (
              <tr>
                <td
                  colSpan={
                    effectiveColumns.length +
                    (expandable?.expandedRowRender ? 1 : 0)
                  }
                >
                  <div className="flex items-center justify-center py-8">
                    <Loader2 className="h-6 w-6 animate-spin text-[var(--accent)]" />
                  </div>
                </td>
              </tr>
            ) : dataSource.length === 0 ? (
              <tr>
                <td
                  colSpan={
                    effectiveColumns.length +
                    (expandable?.expandedRowRender ? 1 : 0)
                  }
                >
                  {locale?.emptyText ?? <Empty />}
                </td>
              </tr>
            ) : (
              <>
                {virtual && virtualTopH > 0 && (
                  <tr style={{ height: virtualTopH }}>
                    <td
                      colSpan={effectiveColumns.length}
                      style={{ padding: 0, borderWidth: 0 }}
                    />
                  </tr>
                )}
                {renderRows(
                  renderData,
                  effectiveColumns,
                  rowKey,
                  expandable,
                  expandedKeys,
                  toggleExpand,
                  0,
                  sizeClass,
                  bordered,
                  rowClassName,
                  dndMergedOnRow ?? onRow,
                  virtual ? { v: startIdx } : undefined,
                )}
                {virtual && virtualBottomH > 0 && (
                  <tr style={{ height: virtualBottomH }}>
                    <td
                      colSpan={effectiveColumns.length}
                      style={{ padding: 0, borderWidth: 0 }}
                    />
                  </tr>
                )}
              </>
            )}
          </tbody>
        </table>
      </div>
      {summary ? <div className="mt-2">{summary()}</div> : null}
      {!virtual && pagination !== false && pagination ? (
        <div className="mt-4 flex justify-end">
          <Pagination
            {...pagination}
            total={pagination.total ?? sortedData.length}
            current={paginationState.current}
            pageSize={paginationState.pageSize}
            onChange={(page, pageSize) => {
              setPaginationState({ current: page, pageSize });
              pagination.onChange?.(page, pageSize);
              onChange?.({ current: page, pageSize });
            }}
          />
        </div>
      ) : null}
    </div>
  );
}

/* Re-export types for convenience */
export type TableColumnsType<T = Record<string, unknown>> = TableColumn<T>[];
