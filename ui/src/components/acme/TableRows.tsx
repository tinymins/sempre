import type { ReactNode } from "react";
import type { TableColumn, TableProps } from "./Table.types";
import { cn } from "./utils";

export function colKey(
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
export function getNestedValue(
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

export function getKey<T>(
  record: T,
  rowKey: string | ((r: T, index: number) => string),
  idx: number,
): string {
  if (typeof rowKey === "function") return rowKey(record, idx);
  const val = (record as Record<string, unknown>)[rowKey];
  return val != null ? String(val) : String(idx);
}

/* ─── Tree Row Renderer ─── */
export function renderRows<T>(
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
