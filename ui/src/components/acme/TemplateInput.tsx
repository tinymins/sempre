import {
  autoUpdate,
  FloatingPortal,
  flip,
  offset,
  shift,
  useFloating,
  useMergeRefs,
} from "@floating-ui/react";
import {
  type ChangeEvent,
  forwardRef,
  type KeyboardEvent,
  useEffect,
  useRef,
  useState,
} from "react";
import { FloatingVibrancy } from "./FloatingVibrancy";
import { cn } from "./utils";

export interface TemplateVariable {
  /** Variable key (used in {{key}}) */
  key: string;
  /** Display label shown in dropdown */
  label: string;
}

interface JinjaKeyword {
  key: string;
  display: string;
  snippet: string;
  /** Cursor position counted from the end of the snippet */
  cursorFromEnd: number;
  /** Optional closing tag appended after the cursor (for paired blocks) */
  closing?: string;
}

const JINJA2_KEYWORDS: JinjaKeyword[] = [
  {
    key: "if",
    snippet: "{% if  %}",
    display: "{% if … %}…{% endif %}",
    cursorFromEnd: 3,
    closing: "{% endif %}",
  },
  {
    key: "elif",
    snippet: "{% elif  %}",
    display: "{% elif … %}",
    cursorFromEnd: 3,
  },
  {
    key: "else",
    snippet: "{% else %}",
    display: "{% else %}",
    cursorFromEnd: 0,
  },
  {
    key: "endif",
    snippet: "{% endif %}",
    display: "{% endif %}",
    cursorFromEnd: 0,
  },
  {
    key: "for",
    snippet: "{% for  in  %}",
    display: "{% for … in … %}…{% endfor %}",
    cursorFromEnd: 7,
    closing: "{% endfor %}",
  },
  {
    key: "endfor",
    snippet: "{% endfor %}",
    display: "{% endfor %}",
    cursorFromEnd: 0,
  },
  {
    key: "set",
    snippet: "{% set  =  %}",
    display: "{% set … = … %}",
    cursorFromEnd: 6,
  },
  {
    key: "filter",
    snippet: "{% filter  %}",
    display: "{% filter … %}…{% endfilter %}",
    cursorFromEnd: 3,
    closing: "{% endfilter %}",
  },
  {
    key: "endfilter",
    snippet: "{% endfilter %}",
    display: "{% endfilter %}",
    cursorFromEnd: 0,
  },
  {
    key: "raw",
    snippet: "{% raw %}",
    display: "{% raw %}…{% endraw %}",
    cursorFromEnd: 0,
    closing: "{% endraw %}",
  },
  {
    key: "endraw",
    snippet: "{% endraw %}",
    display: "{% endraw %}",
    cursorFromEnd: 0,
  },
];

type TriggerMode = "var" | "jinja";

export interface TemplateInputProps {
  value?: string;
  onChange?: (e: ChangeEvent<HTMLInputElement>) => void;
  placeholder?: string;
  className?: string;
  /** Available template variables for autocomplete */
  vars: TemplateVariable[];
  /** Enable Jinja2 keyword autocomplete on {%  (default: true) */
  jinja?: boolean;
  /** Disabled state */
  disabled?: boolean;
}

export const TemplateInput = forwardRef<HTMLInputElement, TemplateInputProps>(
  (
    {
      value = "",
      onChange,
      placeholder,
      className,
      vars,
      jinja = true,
      disabled,
    },
    _ref,
  ) => {
    const inputRef = useRef<HTMLInputElement>(null);
    const listRef = useRef<HTMLDivElement>(null);
    const [open, setOpen] = useState(false);
    const [activeIdx, setActiveIdx] = useState(0);
    const [triggerPos, setTriggerPos] = useState(-1);
    const [mode, setMode] = useState<TriggerMode>("var");

    const { refs, floatingStyles } = useFloating({
      open,
      onOpenChange: setOpen,
      placement: "bottom-start",
      middleware: [offset(4), flip(), shift({ padding: 8 })],
      whileElementsMounted: autoUpdate,
    });

    function getCursor() {
      return inputRef.current?.selectionStart ?? value.length;
    }

    // Compute filtered items based on current mode
    const rawQuery =
      triggerPos >= 0
        ? value.slice(triggerPos + 2, getCursor()) // both {{ and {% are 2 chars
        : "";
    const query = mode === "jinja" ? rawQuery.trimStart() : rawQuery;

    const filteredVars =
      mode === "var"
        ? vars.filter((v) =>
            v.key.toLowerCase().startsWith(query.toLowerCase()),
          )
        : [];
    const filteredJinja =
      mode === "jinja"
        ? JINJA2_KEYWORDS.filter((k) =>
            k.key.toLowerCase().startsWith(query.toLowerCase()),
          )
        : [];
    const hasItems =
      mode === "var" ? filteredVars.length > 0 : filteredJinja.length > 0;
    const itemCount =
      mode === "var" ? filteredVars.length : filteredJinja.length;

    // Scroll active item into view
    useEffect(() => {
      if (!open || !listRef.current) return;
      const activeEl = listRef.current.children[activeIdx] as
        | HTMLElement
        | undefined;
      activeEl?.scrollIntoView({ block: "nearest" });
    }, [activeIdx, open]);

    function detectTrigger(before: string): {
      detected: TriggerMode;
      pos: number;
    } | null {
      const lastVar = before.lastIndexOf("{{");
      const lastVarClose = before.lastIndexOf("}}");
      const lastJinja = before.lastIndexOf("{%");
      const lastJinjaClose = before.lastIndexOf("%}");

      const varOpen = lastVar >= 0 && lastVar > lastVarClose;
      const jinjaOpen = jinja && lastJinja >= 0 && lastJinja > lastJinjaClose;

      if (!varOpen && !jinjaOpen) return null;
      if (varOpen && !jinjaOpen) return { detected: "var", pos: lastVar };
      if (!varOpen && jinjaOpen) return { detected: "jinja", pos: lastJinja };
      // Both open — pick the later one (closer to cursor)
      return lastVar > lastJinja
        ? { detected: "var", pos: lastVar }
        : { detected: "jinja", pos: lastJinja };
    }

    function handleChange(e: ChangeEvent<HTMLInputElement>) {
      onChange?.(e);
      const val = e.target.value;
      const cursor = e.target.selectionStart ?? val.length;
      const before = val.slice(0, cursor);

      const result = detectTrigger(before);
      if (result) {
        const partial = before.slice(result.pos + 2);
        const trimmed =
          result.detected === "jinja" ? partial.trimStart() : partial;
        if (/^\w*$/.test(trimmed)) {
          setMode(result.detected);
          setTriggerPos(result.pos);
          setActiveIdx(0);
          setOpen(true);
          return;
        }
      }
      setOpen(false);
      setTriggerPos(-1);
    }

    function fireChange(newValue: string) {
      const nativeEvent = new Event("input", { bubbles: true });
      Object.defineProperty(nativeEvent, "target", {
        value: { value: newValue },
      });
      onChange?.(nativeEvent as unknown as ChangeEvent<HTMLInputElement>);
    }

    function insertVar(varKey: string) {
      const cursor = getCursor();
      const prefix = value.slice(0, triggerPos);
      const suffix = value.slice(cursor);
      const insertion = `{{${varKey}}}`;
      const newValue = prefix + insertion + suffix;

      fireChange(newValue);
      setOpen(false);
      setTriggerPos(-1);

      const newCursor = prefix.length + insertion.length;
      requestAnimationFrame(() => {
        inputRef.current?.focus();
        inputRef.current?.setSelectionRange(newCursor, newCursor);
      });
    }

    function insertJinja(keyword: JinjaKeyword) {
      const cursor = getCursor();
      const prefix = value.slice(0, triggerPos);
      const suffix = value.slice(cursor);
      const closingPart = keyword.closing ?? "";
      const newValue = prefix + keyword.snippet + closingPart + suffix;

      fireChange(newValue);
      setOpen(false);
      setTriggerPos(-1);

      const newCursor =
        prefix.length + keyword.snippet.length - keyword.cursorFromEnd;
      requestAnimationFrame(() => {
        inputRef.current?.focus();
        inputRef.current?.setSelectionRange(newCursor, newCursor);
      });
    }

    function handleSelect(idx: number) {
      if (mode === "var") {
        insertVar(filteredVars[idx].key);
      } else {
        insertJinja(filteredJinja[idx]);
      }
    }

    function handleKeyDown(e: KeyboardEvent<HTMLInputElement>) {
      if (!open || !hasItems) return;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setActiveIdx((i) => (i + 1) % itemCount);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setActiveIdx((i) => (i - 1 + itemCount) % itemCount);
      } else if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        handleSelect(activeIdx);
      } else if (e.key === "Escape") {
        setOpen(false);
      }
    }

    function handleBlur() {
      setTimeout(() => setOpen(false), 150);
    }

    return (
      <div className="relative w-full">
        <input
          ref={useMergeRefs([inputRef, refs.setReference])}
          value={value}
          onChange={handleChange}
          onKeyDown={handleKeyDown}
          onBlur={handleBlur}
          placeholder={placeholder}
          disabled={disabled}
          className={cn(
            "w-full rounded-md border bg-white/70 dark:bg-white/[0.03] backdrop-blur-sm px-3 py-1.5 text-sm h-8 font-mono outline-none transition-colors",
            "border-black/[0.08] dark:border-white/[0.1]",
            "focus:border-[var(--accent)] focus:ring-1 focus:ring-[var(--accent)]",
            "placeholder:text-[var(--text-muted)]",
            "disabled:cursor-not-allowed disabled:opacity-50",
            className,
          )}
        />
        {open && hasItems && (
          <FloatingPortal>
            <div
              ref={refs.setFloating}
              style={{
                ...floatingStyles,
                scrollbarWidth: "thin",
                scrollbarColor: "rgb(128 128 128 / 0.35) transparent",
              }}
              className="z-[9999] rounded-md overflow-hidden bg-white/90 dark:bg-[rgba(15,15,25,0.9)] backdrop-blur-xl border border-black/[0.06] dark:border-white/[0.08] shadow-lg py-1 text-sm max-h-60 overflow-auto"
            >
              <FloatingVibrancy />
              <div ref={listRef} className="relative">
                {mode === "var"
                  ? filteredVars.map((item, idx) => (
                      <div
                        key={item.key}
                        role="option"
                        tabIndex={-1}
                        aria-selected={idx === activeIdx}
                        className={cn(
                          "px-3 py-1.5 cursor-pointer flex items-center gap-3",
                          idx === activeIdx
                            ? "bg-sky-50 dark:bg-sky-900/30 text-sky-600 dark:text-sky-400"
                            : "text-fg-secondary hover:bg-fill-tertiary",
                        )}
                        onMouseEnter={() => setActiveIdx(idx)}
                        onMouseDown={(e) => {
                          e.preventDefault();
                          insertVar(item.key);
                        }}
                      >
                        <code className="text-sky-600 dark:text-sky-400 text-xs">
                          {`{{${item.key}}}`}
                        </code>
                        <span className="text-xs text-fg-muted">
                          {item.label}
                        </span>
                      </div>
                    ))
                  : filteredJinja.map((kw, idx) => (
                      <div
                        key={kw.key}
                        role="option"
                        tabIndex={-1}
                        aria-selected={idx === activeIdx}
                        className={cn(
                          "px-3 py-1.5 cursor-pointer flex items-center gap-3",
                          idx === activeIdx
                            ? "bg-violet-50 dark:bg-violet-900/30 text-violet-600 dark:text-violet-400"
                            : "text-fg-secondary hover:bg-fill-tertiary",
                        )}
                        onMouseEnter={() => setActiveIdx(idx)}
                        onMouseDown={(e) => {
                          e.preventDefault();
                          insertJinja(kw);
                        }}
                      >
                        <code className="text-violet-600 dark:text-violet-400 text-xs">
                          {kw.key}
                        </code>
                        <span className="text-xs text-fg-muted">
                          {kw.display}
                        </span>
                      </div>
                    ))}
              </div>
            </div>
          </FloatingPortal>
        )}
      </div>
    );
  },
);
TemplateInput.displayName = "TemplateInput";
