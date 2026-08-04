import { createContext, useContext } from "react";

export interface UIContextValue {
  /** Desktop wallpaper URL (null = no wallpaper, skip vibrancy) */
  wallpaperUrl: string | null;
  /** "light" | "dark" */
  theme: "light" | "dark";
  /** Window blur radius in px (e.g. 12) */
  windowBlur: number;
  /** Window opacity 0–100 (e.g. 85) */
  windowOpacity: number;
}

const defaultValue: UIContextValue = {
  wallpaperUrl: null,
  theme: "light",
  windowBlur: 12,
  windowOpacity: 85,
};

export const UIContext = createContext<UIContextValue>(defaultValue);

export function useUIContext() {
  return useContext(UIContext);
}
