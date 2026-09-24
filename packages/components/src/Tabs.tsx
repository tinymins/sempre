import { type ReactNode, useState } from "react";
import { LeftTabs, LineTabs, PillTabs, SegmentTabs } from "./TabsVariants";

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
