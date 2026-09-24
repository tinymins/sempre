import data from "@emoji-mart/data";
import Picker from "@emoji-mart/react";
import {
  autoUpdate,
  FloatingPortal,
  flip,
  offset,
  type Placement,
  shift,
  useClick,
  useDismiss,
  useFloating,
  useInteractions,
} from "@floating-ui/react";
import { Smile } from "lucide-react";
import React, { type MouseEvent, useState } from "react";
import { cn } from "./utils";

export interface EmojiPickerProps {
  /** Currently selected emoji character */
  value?: string | null;
  /** Called when user selects an emoji */
  onChange?: (emoji: string) => void;
  /** If provided, a "clear" button is rendered next to the trigger when a value is set */
  onClear?: () => void;
  /** Disables interaction */
  disabled?: boolean;
  /** Additional className for the trigger button */
  className?: string;
  /** Floating panel placement */
  placement?: Placement;
  /** Emoji picker theme */
  theme?: "light" | "dark" | "auto";
  /** Emoji picker locale */
  locale?: string;
  /** Native title attribute on trigger button */
  title?: string;
  /** Stop click event propagation on the trigger (e.g. inside a menu item) */
  stopPropagation?: boolean;
  /** Clear button label */
  clearLabel?: string;
  /**
   * "default" — full box with border, for use in forms.
   * "icon" — bare icon only, no border/bg, for use inside menu icon slots.
   */
  variant?: "default" | "icon";
  /**
   * Override the HTML element used for the trigger.
   * Use "span" when the picker is nested inside another interactive element
   * (e.g. a <button>) to avoid invalid nested-button HTML.
   */
  triggerAs?: "button" | "span";
}

export function EmojiPicker({
  value,
  onChange,
  onClear,
  disabled,
  className,
  placement = "bottom-start",
  theme = "auto",
  locale,
  title,
  stopPropagation = false,
  clearLabel,
  variant = "default",
  triggerAs = "button",
}: EmojiPickerProps) {
  const [open, setOpen] = useState(false);

  const { refs, floatingStyles, context } = useFloating({
    open,
    onOpenChange: setOpen,
    placement,
    middleware: [offset(6), flip(), shift({ padding: 5 })],
    whileElementsMounted: autoUpdate,
  });

  const click = useClick(context, { enabled: !disabled });
  const dismiss = useDismiss(context);
  const { getReferenceProps, getFloatingProps } = useInteractions([
    click,
    dismiss,
  ]);

  const referenceProps = getReferenceProps();

  const handleTriggerClick = (e: MouseEvent<HTMLElement>) => {
    if (stopPropagation) e.stopPropagation();
    if (typeof referenceProps.onClick === "function") {
      referenceProps.onClick(e);
    }
  };

  const handleTriggerKeyDown = (e: React.KeyboardEvent<HTMLElement>) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      if (!disabled) setOpen((prev) => !prev);
    }
  };

  const iconTriggerClass = cn(
    "inline-flex items-center justify-center appearance-none bg-transparent border-0 p-1 aspect-square rounded cursor-pointer transition-colors hover:bg-black/[0.04] dark:hover:bg-white/[0.06]",
    disabled && "cursor-not-allowed opacity-50",
    className,
  );

  const iconTriggerContent = value ? (
    <span className="text-[1em] leading-none">{value}</span>
  ) : (
    <Smile className="w-[1em] h-[1em] text-[var(--text-muted)]" />
  );

  if (variant === "icon") {
    const TriggerIcon =
      triggerAs === "span" ? (
        <span
          ref={refs.setReference as React.Ref<HTMLSpanElement>}
          role="button"
          tabIndex={disabled ? undefined : 0}
          title={title}
          aria-disabled={disabled}
          className={iconTriggerClass}
          {...referenceProps}
          onClick={handleTriggerClick}
          onKeyDown={handleTriggerKeyDown}
        >
          {iconTriggerContent}
        </span>
      ) : (
        <button
          type="button"
          ref={refs.setReference}
          title={title}
          disabled={disabled}
          className={iconTriggerClass}
          {...referenceProps}
          onClick={handleTriggerClick}
        >
          {iconTriggerContent}
        </button>
      );

    return (
      <>
        {TriggerIcon}
        {open && (
          <FloatingPortal>
            <div
              ref={refs.setFloating}
              style={floatingStyles}
              className="z-[9999]"
              {...getFloatingProps()}
            >
              <Picker
                data={data}
                locale={locale}
                theme={theme}
                previewPosition="none"
                skinTonePosition="none"
                onEmojiSelect={(emoji: { native: string }) => {
                  onChange?.(emoji.native);
                  setOpen(false);
                }}
              />
            </div>
          </FloatingPortal>
        )}
      </>
    );
  }

  return (
    <div className="flex items-center gap-2">
      <button
        type="button"
        ref={refs.setReference}
        title={title}
        disabled={disabled}
        className={cn(
          "flex h-9 w-9 items-center justify-center rounded-lg border text-xl transition-colors",
          open
            ? "border-[var(--accent)] bg-[var(--accent)]/10"
            : "border-[var(--glass-border)] bg-surface-elevated hover:border-[var(--glass-border)] ",
          disabled && "cursor-not-allowed opacity-50",
          className,
        )}
        {...referenceProps}
        onClick={handleTriggerClick}
      >
        {value ? (
          <span className="text-base leading-none">{value}</span>
        ) : (
          <Smile className="h-4 w-4 text-[var(--text-muted)]" />
        )}
      </button>
      {onClear && value && (
        <button
          type="button"
          onClick={() => onClear()}
          className="text-xs text-fg-muted hover:text-fg-secondary"
        >
          {clearLabel}
        </button>
      )}
      {open && (
        <FloatingPortal>
          <div
            ref={refs.setFloating}
            style={floatingStyles}
            className="z-[9999]"
            {...getFloatingProps()}
          >
            <Picker
              data={data}
              locale={locale}
              theme={theme}
              previewPosition="none"
              skinTonePosition="none"
              onEmojiSelect={(emoji: { native: string }) => {
                onChange?.(emoji.native);
                setOpen(false);
              }}
            />
          </div>
        </FloatingPortal>
      )}
    </div>
  );
}
