import type { ReactNode } from "react";
import { cn } from "./utils";

export interface DescriptionsItem {
  key?: string;
  label: ReactNode;
  children: ReactNode;
  span?: number;
}

export interface DescriptionsProps {
  /** Title */
  title?: ReactNode;
  /** Extra content */
  extra?: ReactNode;
  /** Items */
  items?: DescriptionsItem[];
  /** Number of columns */
  column?: number;
  /** Bordered layout */
  bordered?: boolean;
  /** Size */
  size?: "small" | "middle" | "default";
  /** Layout */
  layout?: "horizontal" | "vertical";
  /** Label style */
  labelStyle?: React.CSSProperties;
  /** Content style */
  contentStyle?: React.CSSProperties;
  className?: string;
  children?: ReactNode;
}

export function Descriptions({
  title,
  extra,
  items = [],
  column = 3,
  bordered = false,
  size = "default",
  layout: _layout = "horizontal",
  labelStyle,
  contentStyle,
  className,
}: DescriptionsProps) {
  const padClass = {
    small: "px-2 py-1 text-xs",
    middle: "px-3 py-2 text-sm",
    default: "px-4 py-3 text-sm",
  }[size];

  // Distribute items into rows
  const rows: DescriptionsItem[][] = [];
  let currentRow: DescriptionsItem[] = [];
  let currentSpan = 0;

  for (const item of items) {
    const span = item.span ?? 1;
    if (currentSpan + span > column && currentRow.length > 0) {
      rows.push(currentRow);
      currentRow = [];
      currentSpan = 0;
    }
    currentRow.push(item);
    currentSpan += span;
    if (currentSpan >= column) {
      rows.push(currentRow);
      currentRow = [];
      currentSpan = 0;
    }
  }
  if (currentRow.length > 0) rows.push(currentRow);

  if (bordered) {
    return (
      <div className={cn("w-full", className)}>
        {(title || extra) && (
          <div className="flex items-center justify-between mb-3">
            {title ? (
              <h3 className="text-base font-medium text-[var(--text-primary)]">
                {title}
              </h3>
            ) : null}
            {extra}
          </div>
        )}
        <table className="w-full border-collapse border border-[var(--border-base)] rounded-lg overflow-hidden">
          <tbody>
            {rows.map((row, ri) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: rows are index-based
              <tr key={ri}>
                {row.map((item, ci) => (
                  <Fragment key={item.key ?? ci}>
                    <th
                      className={cn(
                        padClass,
                        "text-left font-normal text-[var(--text-muted)] bg-[var(--fill-tertiary)] border border-[var(--border-base)] whitespace-nowrap",
                      )}
                      style={labelStyle}
                    >
                      {item.label}
                    </th>
                    <td
                      className={cn(
                        padClass,
                        "text-[var(--text-secondary)] bg-[var(--bg-elevated)] border border-[var(--border-base)]",
                      )}
                      colSpan={
                        (item.span ?? 1) > 1
                          ? (item.span ?? 1) * 2 - 1
                          : undefined
                      }
                      style={contentStyle}
                    >
                      {item.children}
                    </td>
                  </Fragment>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  }

  // Non-bordered (default)
  return (
    <div className={cn("w-full", className)}>
      {(title || extra) && (
        <div className="flex items-center justify-between mb-3">
          {title ? (
            <h3 className="text-base font-medium text-[var(--text-primary)]">
              {title}
            </h3>
          ) : null}
          {extra}
        </div>
      )}
      <div
        className="grid gap-x-6 gap-y-2"
        style={{ gridTemplateColumns: `repeat(${column}, minmax(0, 1fr))` }}
      >
        {items.map((item, i) => (
          <div
            key={item.key ?? i}
            style={item.span ? { gridColumn: `span ${item.span}` } : undefined}
          >
            <div
              className="text-sm text-[var(--text-muted)] mb-0.5"
              style={labelStyle}
            >
              {item.label}
            </div>
            <div
              className="text-sm text-[var(--text-primary)]"
              style={contentStyle}
            >
              {item.children}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// Need Fragment import
import { Fragment } from "react";
