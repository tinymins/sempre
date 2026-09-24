import data from "@emoji-mart/data";
import Picker from "@emoji-mart/react";

export interface InlineEmojiPickerProps {
  /** Called when user selects an emoji */
  onSelect?: (emoji: string) => void;
  /** Picker theme */
  theme?: "light" | "dark" | "auto";
  /** Picker locale */
  locale?: string;
}

/**
 * Renders the emoji-mart picker inline (no trigger button / floating wrapper).
 * Use inside a tab panel, modal body, or any container that needs an embedded picker.
 */
export function InlineEmojiPicker({
  onSelect,
  theme = "auto",
  locale,
}: InlineEmojiPickerProps) {
  return (
    <Picker
      data={data}
      onEmojiSelect={(e: { native: string }) => onSelect?.(e.native)}
      theme={theme}
      locale={locale}
      previewPosition="none"
      skinTonePosition="none"
      set="native"
      perLine={9}
    />
  );
}
