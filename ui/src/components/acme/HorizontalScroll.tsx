import { type HTMLAttributes, type ReactNode, useEffect, useRef } from "react";
import { cn } from "./utils";

export interface HorizontalScrollProps
  extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  children: ReactNode;
  /** Extra class applied to the inner flex container */
  innerClassName?: string;
}

/**
 * 横向滚动容器。
 * 鼠标滚轮（垂直方向）会被拦截并转换为横向滚动，避免触发页面纵向滚动。
 * 注意：必须用原生 addEventListener({ passive: false }) 才能调用 preventDefault()，
 * React 的 onWheel 是 passive 的，无法阻止默认行为。
 */
export function HorizontalScroll({
  children,
  className,
  innerClassName,
  ...props
}: HorizontalScrollProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    const onWheel = (e: WheelEvent) => {
      const hasOverflow = el.scrollWidth > el.clientWidth;
      if (!hasOverflow) return;
      e.preventDefault();
      el.scrollLeft += e.deltaY + e.deltaX;
    };

    el.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      el.removeEventListener("wheel", onWheel);
    };
  }, []);

  return (
    <div ref={ref} className={cn("overflow-x-auto", className)} {...props}>
      <div className={cn("flex", innerClassName)}>{children}</div>
    </div>
  );
}
