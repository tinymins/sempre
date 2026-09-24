import { ChevronDown, ChevronRight } from "lucide-react";
import { type ReactNode, useState } from "react";
import { cn } from "./utils";

export interface MenuItem {
  key: string;
  label: ReactNode;
  icon?: ReactNode;
  disabled?: boolean;
  danger?: boolean;
  children?: MenuItem[];
  type?: "group" | "divider";
}

export interface MenuProps {
  /** Menu items */
  items?: MenuItem[];
  /** Selected keys */
  selectedKeys?: string[];
  /** Default selected keys */
  defaultSelectedKeys?: string[];
  /** Open keys (for sub-menus) */
  openKeys?: string[];
  /** Default open keys */
  defaultOpenKeys?: string[];
  /** Select callback */
  onSelect?: (info: { key: string }) => void;
  /** Click handler (antd compat) */
  onClick?: (info: { key: string }) => void;
  /** Open change callback */
  onOpenChange?: (openKeys: string[]) => void;
  /** Mode */
  mode?: "vertical" | "horizontal" | "inline";
  /** Theme */
  theme?: "light" | "dark";
  /** Collapsed state */
  inlineCollapsed?: boolean;
  /** Indent level */
  inlineIndent?: number;
  className?: string;
  style?: React.CSSProperties;
}

export function Menu({
  items = [],
  selectedKeys: selectedKeysProp,
  defaultSelectedKeys = [],
  openKeys: openKeysProp,
  defaultOpenKeys = [],
  onSelect,
  onClick,
  onOpenChange,
  mode: _mode = "inline",
  inlineCollapsed = false,
  inlineIndent = 24,
  className,
  style,
}: MenuProps) {
  const [internalSelected, setInternalSelected] =
    useState<string[]>(defaultSelectedKeys);
  const [internalOpen, setInternalOpen] = useState<string[]>(defaultOpenKeys);

  const selectedKeys = selectedKeysProp ?? internalSelected;
  const openKeys = openKeysProp ?? internalOpen;

  const handleSelect = (key: string) => {
    if (selectedKeysProp === undefined) setInternalSelected([key]);
    (onSelect ?? onClick)?.({ key });
  };

  const toggleOpen = (key: string) => {
    const next = openKeys.includes(key)
      ? openKeys.filter((k) => k !== key)
      : [...openKeys, key];
    if (openKeysProp === undefined) setInternalOpen(next);
    onOpenChange?.(next);
  };

  return (
    <nav className={cn("w-full select-none", className)} style={style}>
      <ul className="list-none p-0 m-0">
        {items.map((item) => (
          <MenuItemRenderer
            key={item.key}
            item={item}
            level={0}
            selectedKeys={selectedKeys}
            openKeys={openKeys}
            onSelect={handleSelect}
            onToggleOpen={toggleOpen}
            collapsed={inlineCollapsed}
            indent={inlineIndent}
          />
        ))}
      </ul>
    </nav>
  );
}

function MenuItemRenderer({
  item,
  level,
  selectedKeys,
  openKeys,
  onSelect,
  onToggleOpen,
  collapsed,
  indent,
}: {
  item: MenuItem;
  level: number;
  selectedKeys: string[];
  openKeys: string[];
  onSelect: (key: string) => void;
  onToggleOpen: (key: string) => void;
  collapsed: boolean;
  indent: number;
}) {
  if (item.type === "divider") {
    return <li className="my-1 h-px bg-black/[0.06] dark:bg-white/[0.08]" />;
  }

  if (item.type === "group") {
    return (
      <li>
        <div className="px-3 py-1.5 text-xs font-medium text-[var(--text-muted)] uppercase tracking-wider">
          {!collapsed && item.label}
        </div>
        <ul className="list-none p-0 m-0">
          {item.children?.map((child) => (
            <MenuItemRenderer
              key={child.key}
              item={child}
              level={level}
              selectedKeys={selectedKeys}
              openKeys={openKeys}
              onSelect={onSelect}
              onToggleOpen={onToggleOpen}
              collapsed={collapsed}
              indent={indent}
            />
          ))}
        </ul>
      </li>
    );
  }

  const isSelected = selectedKeys.includes(item.key);
  const hasChildren = item.children && item.children.length > 0;
  const isOpen = openKeys.includes(item.key);

  return (
    <li>
      <button
        type="button"
        disabled={item.disabled}
        className={cn(
          "flex items-center gap-2 w-full rounded-md px-3 py-2 text-sm transition-colors cursor-pointer",
          isSelected
            ? "bg-gradient-to-r from-[var(--accent-subtle)] to-transparent text-[var(--accent)] font-medium border-l-2 border-[var(--accent)]"
            : "text-[var(--text-secondary,#475569)] hover:bg-black/[0.04] dark:hover:bg-white/[0.04] hover:text-[var(--text-primary)]",
          item.disabled && "opacity-50 !cursor-not-allowed",
          item.danger && "text-red-500 hover:bg-red-50 dark:hover:bg-red-950",
        )}
        style={!collapsed ? { paddingLeft: level * indent + 12 } : undefined}
        onClick={() => {
          if (hasChildren) {
            onToggleOpen(item.key);
          } else {
            onSelect(item.key);
          }
        }}
      >
        {item.icon ? (
          <span className="shrink-0 inline-flex w-5 justify-center [&>svg]:w-[1em] [&>svg]:h-[1em]">
            {item.icon}
          </span>
        ) : null}
        {!collapsed && (
          <span className="flex-1 text-left truncate">{item.label}</span>
        )}
        {!collapsed && hasChildren ? (
          <span className="shrink-0">
            {isOpen ? (
              <ChevronDown className="h-4 w-4 text-[var(--text-muted)]" />
            ) : (
              <ChevronRight className="h-4 w-4 text-[var(--text-muted)]" />
            )}
          </span>
        ) : null}
      </button>
      {hasChildren && isOpen && !collapsed ? (
        <ul className="list-none p-0 m-0">
          {item.children?.map((child) => (
            <MenuItemRenderer
              key={child.key}
              item={child}
              level={level + 1}
              selectedKeys={selectedKeys}
              openKeys={openKeys}
              onSelect={onSelect}
              onToggleOpen={onToggleOpen}
              collapsed={collapsed}
              indent={indent}
            />
          ))}
        </ul>
      ) : null}
    </li>
  );
}
