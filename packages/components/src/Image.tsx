import { ImageOff } from "lucide-react";
import { type ImgHTMLAttributes, useState } from "react";
import { cn } from "./utils";

export interface ImageProps extends ImgHTMLAttributes<HTMLImageElement> {
  /** Fallback image or element when loading fails */
  fallback?: string;
  /** Preview on click */
  preview?:
    | boolean
    | { visible?: boolean; onVisibleChange?: (visible: boolean) => void };
  /** Width */
  width?: number | string;
  /** Height */
  height?: number | string;
  /** Placeholder while loading */
  placeholder?: React.ReactNode;
  /** Root wrapper class name (antd compat) */
  rootClassName?: string;
}

export function Image({
  src,
  alt,
  fallback,
  preview = false,
  width,
  height,
  placeholder,
  className,
  style,
  ...rest
}: ImageProps) {
  const [error, setError] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [previewVisible, setPreviewVisible] = useState(false);

  const isPreviewable = preview !== false;

  const imgSrc = error && fallback ? fallback : src;

  return (
    <>
      <span
        className={cn(
          "inline-block overflow-hidden relative bg-[var(--bg-skeleton)]",
          !loaded && !error && "animate-pulse",
          isPreviewable && "cursor-zoom-in",
          className,
        )}
        style={{ width, height, ...style }}
      >
        {!loaded && !error && placeholder ? (
          <span className="absolute inset-0 flex items-center justify-center">
            {placeholder}
          </span>
        ) : null}
        {error && !fallback ? (
          <span className="absolute inset-0 flex items-center justify-center text-fg-muted">
            <ImageOff className="h-8 w-8" />
          </span>
        ) : (
          <img
            src={imgSrc}
            alt={alt}
            className={cn(
              "w-full h-full object-cover transition-opacity",
              !loaded && "opacity-0",
            )}
            onLoad={() => setLoaded(true)}
            onError={() => setError(true)}
            onClick={() => {
              if (isPreviewable) {
                if (typeof preview === "object" && preview.onVisibleChange) {
                  preview.onVisibleChange(true);
                } else {
                  setPreviewVisible(true);
                }
              }
            }}
            onKeyDown={(e) => {
              if (isPreviewable && (e.key === "Enter" || e.key === " ")) {
                if (typeof preview === "object" && preview.onVisibleChange) {
                  preview.onVisibleChange(true);
                } else {
                  setPreviewVisible(true);
                }
              }
            }}
            {...rest}
          />
        )}
      </span>
      {/* Simple preview overlay */}
      {isPreviewable &&
      (typeof preview === "object" ? preview.visible : previewVisible) ? (
        <div
          className="fixed inset-0 z-[9999] flex items-center justify-center bg-black/70 cursor-zoom-out"
          role="presentation"
          onClick={() => {
            if (typeof preview === "object" && preview.onVisibleChange) {
              preview.onVisibleChange(false);
            } else {
              setPreviewVisible(false);
            }
          }}
        >
          <img
            src={imgSrc}
            alt={alt}
            className="max-h-[90vh] max-w-[90vw] object-contain"
          />
        </div>
      ) : null}
    </>
  );
}
