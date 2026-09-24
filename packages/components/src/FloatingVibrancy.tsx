import { useLayoutEffect, useRef } from "react";
import { useUIContext } from "./UIContext";

/**
 * Renders a macOS-style vibrancy background layer inside a floating element.
 *
 * Measures the element's viewport position via useLayoutEffect (runs each
 * render) and offsets a viewport-sized wallpaper accordingly — the same
 * approach FloatingWindow uses.
 *
 * Re-renders are triggered naturally by:
 *  - UIContext changes (theme / blur / opacity / wallpaper)
 *  - Parent re-render (e.g. ContextMenu repositions)
 *
 * Place as the first child inside a panel with overflow:hidden + relative.
 */
export function FloatingVibrancy() {
  const { wallpaperUrl, theme, windowBlur, windowOpacity } = useUIContext();
  const outerRef = useRef<HTMLDivElement>(null);
  const wpRef = useRef<HTMLDivElement>(null);

  // Imperatively set wallpaper offset — runs every render to catch repositioning
  useLayoutEffect(() => {
    const el = outerRef.current;
    const wp = wpRef.current;
    if (!el || !wp) return;
    const rect = el.getBoundingClientRect();
    const pad = windowBlur + 4;
    wp.style.top = `${-Math.round(rect.top) - pad}px`;
    wp.style.left = `${-Math.round(rect.left) - pad}px`;
  });

  if (!wallpaperUrl || windowOpacity >= 100) return null;

  const pad = windowBlur + 4;

  return (
    <div
      ref={outerRef}
      className="pointer-events-none absolute inset-0 z-0 overflow-hidden"
      style={{ borderRadius: "inherit" }}
    >
      {/* Viewport-sized wallpaper, offset imperatively */}
      <div
        ref={wpRef}
        style={{
          position: "absolute",
          width: `calc(100vw + ${pad * 2}px)`,
          height: `calc(100vh + ${pad * 2}px)`,
          backgroundImage: `url(${wallpaperUrl})`,
          backgroundSize: "cover",
          backgroundPosition: "center",
          filter: `blur(${windowBlur}px) saturate(1.8)`,
        }}
      />
      {/* Color tint overlay */}
      <div
        className="absolute inset-0"
        style={{
          background:
            theme === "dark"
              ? `rgba(10,10,10,${windowOpacity / 100})`
              : `rgba(255,255,255,${windowOpacity / 100})`,
        }}
      />
    </div>
  );
}
