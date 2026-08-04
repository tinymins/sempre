import type { ReactNode } from "react";
import { cn } from "./utils";

export interface ListItem {
  key?: string;
}

export interface ListProps<T = Record<string, unknown>> {
  /** Data source */
  dataSource?: T[];
  /** Render each item */
  renderItem?: (item: T, index: number) => ReactNode;
  /** Loading state */
  loading?: boolean;
  /** Bordered */
  bordered?: boolean;
  /** Size */
  size?: "small" | "default" | "large";
  /** Header */
  header?: ReactNode;
  /** Footer */
  footer?: ReactNode;
  /** Empty content */
  locale?: { emptyText?: ReactNode };
  /** Split lines between items */
  split?: boolean;
  /** Grid columns (future) */
  grid?: { column?: number; gutter?: number };
  /** Row key */
  rowKey?: string | ((item: T) => string);
  className?: string;
  children?: ReactNode;
}

export function List<T = Record<string, unknown>>({
  dataSource = [],
  renderItem,
  loading = false,
  bordered = false,
  size = "default",
  header,
  footer,
  locale,
  split = true,
  grid,
  rowKey,
  className,
  children,
}: ListProps<T>) {
  const getKey = (item: T, i: number): string => {
    if (typeof rowKey === "function") return rowKey(item);
    if (typeof rowKey === "string")
      return String((item as Record<string, unknown>)[rowKey]);
    return String(
      (item as Record<string, unknown>).key ??
        (item as Record<string, unknown>).id ??
        i,
    );
  };

  const padClass = {
    small: "px-3 py-1.5",
    default: "px-4 py-3",
    large: "px-4 py-4",
  }[size];

  const isEmpty = dataSource.length === 0;

  if (loading) {
    return (
      <div className={cn("flex justify-center py-8", className)}>
        <svg
          className="h-8 w-8 animate-spin text-sky-500"
          viewBox="0 0 24 24"
          fill="none"
        >
          <circle
            className="opacity-25"
            cx="12"
            cy="12"
            r="10"
            stroke="currentColor"
            strokeWidth="4"
          />
          <path
            className="opacity-75"
            fill="currentColor"
            d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
          />
        </svg>
      </div>
    );
  }

  return (
    <div
      className={cn(
        bordered && "border border-border-base rounded-lg overflow-hidden",
        className,
      )}
    >
      {header ? (
        <div
          className={cn(
            padClass,
            "font-medium text-fg-secondary",
            bordered && "border-b border-border-base",
          )}
        >
          {header}
        </div>
      ) : null}
      {children ??
        (isEmpty ? (
          <div className="py-8 text-center text-sm text-fg-muted">
            {locale?.emptyText ?? "暂无数据"}
          </div>
        ) : grid ? (
          <div
            className="grid"
            style={{
              gridTemplateColumns: `repeat(${grid.column ?? 1}, 1fr)`,
              gap: grid.gutter ?? 0,
            }}
          >
            {dataSource.map((item, i) => renderItem?.(item, i))}
          </div>
        ) : (
          <ul className="list-none p-0 m-0">
            {dataSource.map((item, i) => (
              <li
                key={getKey(item, i)}
                className={cn(
                  padClass,
                  split &&
                    i < dataSource.length - 1 &&
                    "border-b border-border-subtle",
                )}
              >
                {renderItem?.(item, i)}
              </li>
            ))}
          </ul>
        ))}
      {footer ? (
        <div
          className={cn(padClass, bordered && "border-t border-border-base")}
        >
          {footer}
        </div>
      ) : null}
    </div>
  );
}

/* ─── List.Item ─── */
export interface ListItemProps {
  children?: ReactNode;
  actions?: ReactNode[];
  extra?: ReactNode;
  className?: string;
  onClick?: () => void;
}

/* ─── List.Item.Meta ─── */
export interface ListItemMetaProps {
  avatar?: ReactNode;
  title?: ReactNode;
  description?: ReactNode;
  className?: string;
}

function ListItemMetaComponent({
  avatar,
  title,
  description,
  className,
}: ListItemMetaProps) {
  return (
    <div className={cn("flex items-start gap-3", className)}>
      {avatar ? <div className="shrink-0">{avatar}</div> : null}
      <div className="flex-1 min-w-0">
        {title ? (
          <div className="text-sm font-medium text-fg-primary">{title}</div>
        ) : null}
        {description ? (
          <div className="mt-2 text-sm text-fg-muted">{description}</div>
        ) : null}
      </div>
    </div>
  );
}

function ListItemComponent({
  children,
  actions,
  extra,
  className,
  onClick,
}: ListItemProps) {
  return (
    <div
      className={cn(
        "flex items-start gap-3",
        onClick && "cursor-pointer hover:bg-fill-tertiary/50",
        className,
      )}
      role={onClick ? "button" : undefined}
      tabIndex={onClick ? 0 : undefined}
      onClick={onClick}
      onKeyDown={
        onClick
          ? (e) => {
              if (e.key === "Enter" || e.key === " ") onClick();
            }
          : undefined
      }
    >
      <div className="flex-1 min-w-0">{children}</div>
      {extra ? <div className="shrink-0">{extra}</div> : null}
      {actions?.length ? (
        <div className="flex items-center gap-2 shrink-0">
          {actions.map((action, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: static action list
            <span key={i}>{action}</span>
          ))}
        </div>
      ) : null}
    </div>
  );
}

ListItemComponent.Meta = ListItemMetaComponent;
List.Item = ListItemComponent;
