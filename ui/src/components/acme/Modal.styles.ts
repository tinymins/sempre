import type { CSSProperties } from "react";
import type { ScaledModalSize } from "./Modal.types";

export const THIN_SCROLLBAR: CSSProperties = {
  scrollbarWidth: "thin",
  scrollbarColor: "rgba(128,128,128,0.4) transparent",
};

interface SizeConfig {
  width: string | number;
  dialogStyle: CSSProperties;
  bodyStyle: CSSProperties;
  containerStyle?: CSSProperties;
}

export const SIZE_CONFIG: Record<ScaledModalSize, SizeConfig> = {
  full: {
    width: "100%",
    dialogStyle: {
      maxWidth: "100%",
      margin: 0,
      padding: 0,
      borderRadius: 0,
      height: "100%",
    },
    bodyStyle: {
      flex: 1,
      minHeight: 0,
      overflowY: "auto",
      overflowX: "hidden",
      ...THIN_SCROLLBAR,
    },
    containerStyle: {
      display: "flex",
      flexDirection: "column",
      height: "100%",
    },
  },
  "almost-full": {
    width: "calc(100% - 48px)",
    dialogStyle: {
      maxWidth: "calc(100% - 48px)",
      height: "calc(100% - 48px)",
    },
    bodyStyle: {
      flex: 1,
      minHeight: 0,
      overflowY: "auto",
      overflowX: "hidden",
      ...THIN_SCROLLBAR,
    },
    containerStyle: {
      display: "flex",
      flexDirection: "column",
      height: "100%",
    },
  },
  large: {
    width: "90%",
    dialogStyle: { maxWidth: 1400 },
    bodyStyle: {
      maxHeight: "calc(100% - 200px)",
      overflowY: "auto",
      overflowX: "hidden",
      ...THIN_SCROLLBAR,
    },
  },
  /** 5% margin on all sides — 90% × 90%, no outer scrollbar */
  inset: {
    width: "90%",
    dialogStyle: {
      maxWidth: "90%",
      height: "90%",
    },
    bodyStyle: {
      flex: 1,
      minHeight: 0,
      overflow: "hidden",
    },
    containerStyle: {
      display: "flex",
      flexDirection: "column",
      height: "100%",
    },
  },
  /** 15% margin top/bottom — 90% × at-most 70%, centered; left-right grid forms pass style={{ height: "70%" }} */
  form: {
    width: "90%",
    dialogStyle: {
      maxWidth: "90%",
      maxHeight: "70%",
    },
    bodyStyle: {
      flex: 1,
      minHeight: 0,
      overflowY: "auto",
      overflowX: "hidden",
      ...THIN_SCROLLBAR,
    },
    containerStyle: {
      display: "flex",
      flexDirection: "column",
      height: "100%",
    },
  },
  default: {
    width: 520,
    dialogStyle: {},
    bodyStyle: {},
  },
};
