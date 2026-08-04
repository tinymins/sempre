import {
  type ReactNode,
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { cn } from "./utils";

export interface TabItem {
  key: string;
  label: ReactNode;
  children?: ReactNode;
  disabled?: boolean;
  icon?: ReactNode;
  closable?: boolean;
}

export interface TabsProps {
  /** Tab items */
  items?: TabItem[];
  /** Active tab key */
  activeKey?: string;
  /** Default active key */
  defaultActiveKey?: string;
  /** Change handler */
  onChange?: (key: string) => void;
  /** Tab bar extra content */
  tabBarExtraContent?: ReactNode;
  /** Content rendered above tab list (useful for tabPosition="left") */
  tabBarHeader?: ReactNode;
  /** Size */
  size?: "small" | "middle" | "large";
  /** Type */
  type?: "line" | "card" | "pill" | "segment";
  /** Tab bar position */
  tabPosition?: "top" | "left";
  /** Centered tabs */
  centered?: boolean;
  /** Destroys inactive panes */
  destroyInactiveTabPane?: boolean;
  className?: string;
  /** Custom class for content area (tabPosition="left" only) */
  contentClassName?: string;
}

export function Tabs({
  items = [],
  activeKey: activeKeyProp,
  defaultActiveKey,
  onChange,
  tabBarExtraContent,
  tabBarHeader,
  size = "middle",
  type = "line",
  tabPosition = "top",
  centered = false,
  destroyInactiveTabPane = false,
  className,
  contentClassName,
}: TabsProps) {
  const [internalKey, setInternalKey] = useState(
    defaultActiveKey ?? items[0]?.key ?? "",
  );
  const activeKey = activeKeyProp ?? internalKey;

  const handleChange = (key: string) => {
    if (activeKeyProp === undefined) setInternalKey(key);
    onChange?.(key);
  };

  const hasChildren = items.some((i) => i.children !== undefined);

  if (tabPosition === "left") {
    return (
      <LeftTabs
        {...{
          items,
          activeKey,
          handleChange,
          tabBarExtraContent,
          tabBarHeader,
          size,
          destroyInactiveTabPane,
          hasChildren,
          className,
          contentClassName,
        }}
      />
    );
  }

  if (type === "pill") {
    return (
      <PillTabs
        {...{
          items,
          activeKey,
          handleChange,
          tabBarExtraContent,
          size,
          centered,
          destroyInactiveTabPane,
          hasChildren,
          className,
        }}
      />
    );
  }

  if (type === "segment") {
    return (
      <SegmentTabs
        {...{
          items,
          activeKey,
          handleChange,
          size,
          destroyInactiveTabPane,
          hasChildren,
          className,
        }}
      />
    );
  }

  return (
    <LineTabs
      {...{
        items,
        activeKey,
        handleChange,
        tabBarExtraContent,
        size,
        type,
        centered,
        destroyInactiveTabPane,
        hasChildren,
        className,
      }}
    />
  );
}

// ── Tab Panels (shared) ──

function TabPanels({
  items,
  activeKey,
  destroyInactiveTabPane,
}: {
  items: TabItem[];
  activeKey: string;
  destroyInactiveTabPane: boolean;
}) {
  return (
    <>
      {items.map((item) => {
        const isActive = activeKey === item.key;
        if (destroyInactiveTabPane && !isActive) return null;
        return (
          <div
            key={item.key}
            className={cn(!isActive && "hidden")}
            role="tabpanel"
          >
            {item.children}
          </div>
        );
      })}
    </>
  );
}

// ── Line / Card Tabs (original) ──

function LineTabs({
  items,
  activeKey,
  handleChange,
  tabBarExtraContent,
  size,
  type,
  centered,
  destroyInactiveTabPane,
  hasChildren,
  className,
}: {
  items: TabItem[];
  activeKey: string;
  handleChange: (key: string) => void;
  tabBarExtraContent?: ReactNode;
  size: "small" | "middle" | "large";
  type: "line" | "card" | "pill" | "segment";
  centered: boolean;
  destroyInactiveTabPane: boolean;
  hasChildren: boolean;
  className?: string;
}) {
  const sizeClass = {
    small: "text-xs px-2 py-1",
    middle: "text-sm px-3 py-2",
    large: "text-base px-4 py-2.5",
  }[size];

  const tabBarRef = useRef<HTMLDivElement>(null);
  const tabRefs = useRef<Map<string, HTMLButtonElement>>(new Map());
  const [indicatorStyle, setIndicatorStyle] = useState({ left: 0, width: 0 });
  const hasInitRef = useRef(false);

  const updateIndicator = useCallback(() => {
    const tabBar = tabBarRef.current;
    const activeTab = tabRefs.current.get(activeKey);
    if (!tabBar || !activeTab) return;
    const barRect = tabBar.getBoundingClientRect();
    const tabRect = activeTab.getBoundingClientRect();
    setIndicatorStyle({
      left: tabRect.left - barRect.left + tabBar.scrollLeft,
      width: tabRect.width,
    });
  }, [activeKey]);

  useLayoutEffect(() => {
    updateIndicator();
    if (!hasInitRef.current) {
      requestAnimationFrame(() => {
        hasInitRef.current = true;
      });
    }
  }, [updateIndicator]);

  return (
    <div className={className}>
      <div className="flex items-center border-b border-black/[0.06] dark:border-white/[0.08] select-none">
        <div
          ref={tabBarRef}
          className={cn(
            "relative flex gap-0 flex-1 overflow-x-auto",
            centered && "justify-center",
          )}
        >
          {items.map((item) => (
            <button
              key={item.key}
              ref={(el) => {
                if (el) tabRefs.current.set(item.key, el);
                else tabRefs.current.delete(item.key);
              }}
              type="button"
              disabled={item.disabled}
              className={cn(
                "relative whitespace-nowrap transition-colors font-medium cursor-pointer",
                sizeClass,
                type === "card" &&
                  "border border-b-0 border-black/[0.06] dark:border-white/[0.08] rounded-t -mb-px",
                activeKey === item.key
                  ? type === "card"
                    ? "bg-white/70 dark:bg-white/[0.03] text-[var(--accent)] border-b-transparent"
                    : "text-[var(--accent)]"
                  : "text-[var(--text-muted)] hover:text-[var(--text-primary)]",
                type === "card" &&
                  activeKey !== item.key &&
                  "bg-black/[0.02] dark:bg-white/[0.03]",
                item.disabled && "opacity-50 !cursor-not-allowed",
              )}
              onClick={() => !item.disabled && handleChange(item.key)}
            >
              <span className="flex items-center gap-1.5 [&>svg]:w-[1em] [&>svg]:h-[1em]">
                {item.icon}
                {item.label}
              </span>
            </button>
          ))}
          {type === "line" && (
            <span
              className="absolute bottom-0 h-0.5 bg-[var(--accent)] rounded-t"
              style={{
                left: indicatorStyle.left,
                width: indicatorStyle.width,
                transition: hasInitRef.current
                  ? "left 0.3s cubic-bezier(0.645, 0.045, 0.355, 1), width 0.3s cubic-bezier(0.645, 0.045, 0.355, 1)"
                  : "none",
              }}
            />
          )}
        </div>
        {tabBarExtraContent ? (
          <div className="shrink-0 ml-auto pl-2">{tabBarExtraContent}</div>
        ) : null}
      </div>
      {hasChildren && (
        <TabPanels
          items={items}
          activeKey={activeKey}
          destroyInactiveTabPane={destroyInactiveTabPane}
        />
      )}
    </div>
  );
}

// ── Pill Tabs ──

function PillTabs({
  items,
  activeKey,
  handleChange,
  tabBarExtraContent,
  size,
  centered,
  destroyInactiveTabPane,
  hasChildren,
  className,
}: {
  items: TabItem[];
  activeKey: string;
  handleChange: (key: string) => void;
  tabBarExtraContent?: ReactNode;
  size: "small" | "middle" | "large";
  centered: boolean;
  destroyInactiveTabPane: boolean;
  hasChildren: boolean;
  className?: string;
}) {
  const sizeClass = {
    small: "px-3 py-1 text-xs",
    middle: "px-4 py-1.5 text-sm",
    large: "px-5 py-2 text-base",
  }[size];

  return (
    <div className={className}>
      <div
        className={cn("flex gap-2 select-none", centered && "justify-center")}
      >
        {items.map((item) => (
          <button
            key={item.key}
            type="button"
            disabled={item.disabled}
            className={cn(
              "rounded-full font-medium transition-colors cursor-pointer",
              sizeClass,
              activeKey === item.key
                ? "bg-[var(--accent)] text-white"
                : "bg-[var(--fill-tertiary)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]",
              item.disabled && "opacity-50 !cursor-not-allowed",
            )}
            onClick={() => !item.disabled && handleChange(item.key)}
          >
            <span className="flex items-center gap-1.5 [&>svg]:w-[1em] [&>svg]:h-[1em]">
              {item.icon}
              {item.label}
            </span>
          </button>
        ))}
        {tabBarExtraContent ? (
          <div className="shrink-0 ml-auto">{tabBarExtraContent}</div>
        ) : null}
      </div>
      {hasChildren && (
        <TabPanels
          items={items}
          activeKey={activeKey}
          destroyInactiveTabPane={destroyInactiveTabPane}
        />
      )}
    </div>
  );
}

// ── Segment Tabs ──

function SegmentTabs({
  items,
  activeKey,
  handleChange,
  size,
  destroyInactiveTabPane,
  hasChildren,
  className,
}: {
  items: TabItem[];
  activeKey: string;
  handleChange: (key: string) => void;
  size: "small" | "middle" | "large";
  destroyInactiveTabPane: boolean;
  hasChildren: boolean;
  className?: string;
}) {
  const sizeClass = {
    small: "px-1.5 py-1 text-xs",
    middle: "px-2 py-1.5 text-xs",
    large: "px-3 py-2 text-sm",
  }[size];

  return (
    <div className={className}>
      <div className="flex gap-0.5 rounded-lg bg-fill-tertiary p-0.5 dark:bg-white/[0.06] select-none">
        {items.map((item) => (
          <button
            key={item.key}
            type="button"
            disabled={item.disabled}
            className={cn(
              "flex flex-1 min-w-0 items-center justify-center gap-2 rounded-md font-medium transition-all cursor-pointer",
              sizeClass,
              activeKey === item.key
                ? "bg-white text-fg-primary shadow-sm dark:bg-white/[0.1]"
                : "text-fg-muted hover:text-fg-secondary",
              item.disabled && "opacity-50 !cursor-not-allowed",
            )}
            onClick={() => !item.disabled && handleChange(item.key)}
          >
            {item.icon}
            <span className="truncate">{item.label}</span>
          </button>
        ))}
      </div>
      {hasChildren && (
        <TabPanels
          items={items}
          activeKey={activeKey}
          destroyInactiveTabPane={destroyInactiveTabPane}
        />
      )}
    </div>
  );
}

// ── Left (Vertical) Tabs ──

function LeftTabs({
  items,
  activeKey,
  handleChange,
  tabBarExtraContent,
  tabBarHeader,
  size,
  destroyInactiveTabPane,
  hasChildren,
  className,
  contentClassName,
}: {
  items: TabItem[];
  activeKey: string;
  handleChange: (key: string) => void;
  tabBarExtraContent?: ReactNode;
  tabBarHeader?: ReactNode;
  size: "small" | "middle" | "large";
  destroyInactiveTabPane: boolean;
  hasChildren: boolean;
  className?: string;
  contentClassName?: string;
}) {
  const sizeClass = {
    small: "px-2.5 py-2 text-xs",
    middle: "px-3 py-2.5 text-sm",
    large: "px-4 py-3 text-base",
  }[size];

  return (
    <div
      className={cn("grid h-full overflow-hidden", className)}
      style={{ gridTemplateColumns: "188px 1fr" }}
    >
      {/* Left: nav sidebar */}
      <div className="border-r border-[var(--border-base)] bg-[var(--sidebar-bg)] flex flex-col overflow-hidden select-none">
        {tabBarHeader}
        <div className="px-2 pt-3 overflow-y-auto flex-1">
          {items.map((item) => (
            <button
              key={item.key}
              type="button"
              disabled={item.disabled}
              onClick={() => !item.disabled && handleChange(item.key)}
              className={cn(
                "w-full flex items-center gap-2.5 rounded-lg mb-0.5 text-left transition-colors cursor-pointer",
                sizeClass,
                activeKey === item.key
                  ? "bg-[var(--accent-subtle)] text-[var(--accent)] font-semibold hover:bg-[var(--accent-subtle-hover)]"
                  : "text-fg-secondary hover:bg-black/[0.08] dark:hover:bg-white/[0.08]",
                item.disabled && "opacity-50 !cursor-not-allowed",
              )}
            >
              {item.icon && <span className="shrink-0">{item.icon}</span>}
              <span className="leading-tight">{item.label}</span>
            </button>
          ))}
          {tabBarExtraContent}
        </div>
      </div>
      {/* Right: content */}
      {hasChildren && (
        <div className="flex flex-col min-h-0">
          <div
            className={cn("flex-1 overflow-y-auto px-6 py-5", contentClassName)}
            style={{ scrollbarWidth: "thin" }}
          >
            <TabPanels
              items={items}
              activeKey={activeKey}
              destroyInactiveTabPane={destroyInactiveTabPane}
            />
          </div>
        </div>
      )}
    </div>
  );
}
