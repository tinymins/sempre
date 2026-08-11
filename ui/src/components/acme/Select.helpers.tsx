import React, { type ReactNode, type RefObject, useEffect, useMemo } from "react";
import type { SelectOption } from "./Select.types";

export const selectSizeMap = {
  small: "min-h-6 py-0 text-xs",
  middle: "min-h-8 py-0.5 text-sm",
  large: "min-h-10 py-1 text-base",
};

export function useVirtualActiveScroll(
  activeIndex: number | null,
  virtual: boolean,
  scrollContainerRef: RefObject<HTMLDivElement | null>,
  itemHeight: number,
  maxHeight: number,
) {
  useEffect(() => {
    if (!virtual || activeIndex === null || !scrollContainerRef.current) return;
    const container = scrollContainerRef.current;
    const top = activeIndex * itemHeight;
    if (top < container.scrollTop) container.scrollTop = top;
    else if (top + itemHeight > container.scrollTop + maxHeight) {
      container.scrollTop = top + itemHeight - maxHeight;
    }
  }, [activeIndex, itemHeight, maxHeight, scrollContainerRef, virtual]);
}

export function useSelectOptions(options: SelectOption[], children: ReactNode) {
  return useMemo(() => {
    if (options.length > 0) return options;
    const result: SelectOption[] = [];
    React.Children.forEach(children, (child) => {
      if (React.isValidElement(child) && child.props) {
        const props = child.props as {
          value?: string | number;
          disabled?: boolean;
          children?: ReactNode;
        };
        if (props.value !== undefined) {
          result.push({
            value: props.value,
            label: props.children ?? String(props.value),
            disabled: props.disabled,
          });
        }
      }
    });
    return result;
  }, [options, children]);
}

export function labelTitle(label: ReactNode) {
  return typeof label === "string" || typeof label === "number"
    ? String(label)
    : undefined;
}
