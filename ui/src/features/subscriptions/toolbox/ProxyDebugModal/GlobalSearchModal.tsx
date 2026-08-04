import { Collapse, Modal, SearchInput, Tag } from "@acme/components";
import type { ProxyDebugStep } from "@acme/types";
import { ChevronDown, ChevronUp } from "lucide-react";
import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";

export interface GlobalSearchModalRef {
  open: () => void;
}

interface SearchMatch {
  sectionKey: string;
  lineIndex: number;
  lineText: string;
  matchStart: number;
  matchEnd: number;
}

interface SearchSection {
  key: string;
  label: string;
  lines: string[];
}

interface Props {
  steps: ProxyDebugStep[];
}

/** Highlight all occurrences of `query` in `text` (case-insensitive). */
function highlightText(text: string, query: string) {
  if (!query) return text;
  const parts: { text: string; highlight: boolean }[] = [];
  const lower = text.toLowerCase();
  const lowerQ = query.toLowerCase();
  let cursor = 0;
  while (cursor < text.length) {
    const idx = lower.indexOf(lowerQ, cursor);
    if (idx === -1) {
      parts.push({ text: text.slice(cursor), highlight: false });
      break;
    }
    if (idx > cursor) {
      parts.push({ text: text.slice(cursor, idx), highlight: false });
    }
    parts.push({ text: text.slice(idx, idx + query.length), highlight: true });
    cursor = idx + query.length;
  }
  return (
    <>
      {parts.map((p) => {
        const key = `${p.highlight ? "h" : "t"}-${p.text.slice(0, 20)}`;
        return p.highlight ? (
          <mark
            key={key}
            className="bg-yellow-300 dark:bg-yellow-600 rounded-sm"
          >
            {p.text}
          </mark>
        ) : (
          <span key={key}>{p.text}</span>
        );
      })}
    </>
  );
}

const GlobalSearchModal = forwardRef<GlobalSearchModalRef, Props>(
  ({ steps }, ref) => {
    const { t } = useTranslation();
    const [visible, setVisible] = useState(false);
    const [query, setQuery] = useState("");
    const [currentIndex, setCurrentIndex] = useState(0);
    const matchRefs = useRef<Map<number, HTMLDivElement>>(new Map());
    const [expandedKeys, setExpandedKeys] = useState<string[]>([]);

    useImperativeHandle(ref, () => ({
      open: () => {
        setVisible(true);
        setQuery("");
        setCurrentIndex(0);
        setExpandedKeys([]);
      },
    }));

    // Build searchable sections from steps
    const sections: SearchSection[] = useMemo(() => {
      const result: SearchSection[] = [];

      // Final config
      const outputStep = steps.find((s) => s.type === "output");
      if (outputStep && outputStep.type === "output") {
        result.push({
          key: "config",
          label: t("proxy.debug.globalSearchFinalConfig"),
          lines: outputStep.data.configOutput.split("\n"),
        });
      }

      // Rule sets
      const ruleSetStep = steps.find((s) => s.type === "rule-sets");
      if (ruleSetStep && ruleSetStep.type === "rule-sets") {
        for (const item of ruleSetStep.data.items) {
          if (item.sampleRules && item.sampleRules.length > 0) {
            result.push({
              key: `ruleset-${item.tag}`,
              label: `${item.group} / ${item.tag}`,
              lines: item.sampleRules,
            });
          }
        }
      }

      return result;
    }, [steps, t]);

    // Compute matches
    const matches: SearchMatch[] = useMemo(() => {
      if (!query || query.length < 2) return [];
      const lowerQ = query.toLowerCase();
      const result: SearchMatch[] = [];
      for (const section of sections) {
        for (let i = 0; i < section.lines.length; i++) {
          const line = section.lines[i];
          const lower = line.toLowerCase();
          let cursor = 0;
          while (cursor < lower.length) {
            const idx = lower.indexOf(lowerQ, cursor);
            if (idx === -1) break;
            result.push({
              sectionKey: section.key,
              lineIndex: i,
              lineText: line,
              matchStart: idx,
              matchEnd: idx + query.length,
            });
            cursor = idx + query.length;
          }
        }
      }
      return result;
    }, [query, sections]);

    // Reset current index and expand all matched sections when matches change
    useEffect(() => {
      setCurrentIndex(0);
      // Expand all sections that have matches by default
      const sectionKeys = new Set<string>();
      for (const m of matches) {
        sectionKeys.add(m.sectionKey);
      }
      setExpandedKeys([...sectionKeys]);
    }, [matches]);

    // Auto-expand section containing current match and scroll to it
    useEffect(() => {
      if (matches.length === 0) return;
      const match = matches[currentIndex];
      if (!match) return;

      // Expand the section
      setExpandedKeys((prev) => {
        if (prev.includes(match.sectionKey)) return prev;
        return [...prev, match.sectionKey];
      });

      // Scroll after render
      requestAnimationFrame(() => {
        const el = matchRefs.current.get(currentIndex);
        el?.scrollIntoView({ behavior: "smooth", block: "center" });
      });
    }, [currentIndex, matches]);

    const goNext = useCallback(() => {
      if (matches.length === 0) return;
      setCurrentIndex((prev) => (prev + 1) % matches.length);
    }, [matches.length]);

    const goPrev = useCallback(() => {
      if (matches.length === 0) return;
      setCurrentIndex((prev) => (prev === 0 ? matches.length - 1 : prev - 1));
    }, [matches.length]);

    // Keyboard shortcuts
    const handleKeyDown = useCallback(
      (e: React.KeyboardEvent) => {
        if (e.key === "Enter" && !e.shiftKey) {
          e.preventDefault();
          goNext();
        } else if (e.key === "Enter" && e.shiftKey) {
          e.preventDefault();
          goPrev();
        }
      },
      [goNext, goPrev],
    );

    // Build match indices grouped by section for rendering
    const matchesBySection = useMemo(() => {
      const map = new Map<string, number[]>();
      for (let i = 0; i < matches.length; i++) {
        const key = matches[i].sectionKey;
        if (!map.has(key)) map.set(key, []);
        map.get(key)?.push(i);
      }
      return map;
    }, [matches]);

    /** Render a section's matching lines */
    const renderSectionMatches = useCallback(
      (sectionKey: string) => {
        const indices = matchesBySection.get(sectionKey);
        if (!indices || indices.length === 0) {
          return (
            <div className="text-xs text-slate-400 py-2">
              {t("proxy.debug.globalSearchNoResults")}
            </div>
          );
        }

        // Deduplicate by lineIndex to avoid showing the same line twice
        const seenLines = new Set<number>();
        const uniqueIndices: number[] = [];
        for (const idx of indices) {
          const m = matches[idx];
          if (!seenLines.has(m.lineIndex)) {
            seenLines.add(m.lineIndex);
            uniqueIndices.push(idx);
          }
        }

        return (
          <div className="flex flex-col gap-0.5 max-h-[400px] overflow-auto">
            {uniqueIndices.map((matchIdx) => {
              const m = matches[matchIdx];
              const isCurrent = matchIdx === currentIndex;
              return (
                <div
                  key={`${m.sectionKey}-${m.lineIndex}`}
                  ref={(el) => {
                    if (el) matchRefs.current.set(matchIdx, el);
                  }}
                  className={`font-mono text-xs px-2 py-0.5 rounded cursor-pointer transition-colors ${
                    isCurrent
                      ? "bg-blue-100 dark:bg-blue-900/40 ring-1 ring-blue-400"
                      : "hover:bg-gray-100 dark:hover:bg-gray-800"
                  }`}
                  onClick={() => setCurrentIndex(matchIdx)}
                >
                  <span className="text-slate-400 select-none mr-2">
                    {m.lineIndex + 1}
                  </span>
                  {highlightText(m.lineText, query)}
                </div>
              );
            })}
          </div>
        );
      },
      [matchesBySection, matches, currentIndex, query, t],
    );

    // Sections that have matches (for collapsible display)
    const collapseItems = useMemo(() => {
      if (!query || query.length < 2) return [];
      return sections
        .filter((s) => matchesBySection.has(s.key))
        .map((section) => {
          const count = matchesBySection.get(section.key)?.length ?? 0;
          return {
            key: section.key,
            label: (
              <div className="flex gap-2 items-center">
                <span className="text-sm">{section.label}</span>
                <Tag color="blue" className="!text-xs">
                  {count}
                </Tag>
              </div>
            ),
            children: renderSectionMatches(section.key),
          };
        });
    }, [sections, query, matchesBySection, renderSectionMatches]);

    return (
      <Modal
        title={t("proxy.debug.globalSearch")}
        open={visible}
        onCancel={() => setVisible(false)}
        footer={null}
        size="large"
        destroyOnClose
      >
        <div className="flex flex-col gap-3" onKeyDown={handleKeyDown}>
          {/* Search input + navigation */}
          <div className="flex items-center gap-2">
            <SearchInput
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("proxy.debug.globalSearchPlaceholder")}
              className="flex-1"
              autoFocus
            />
          </div>

          {/* Match navigation bar */}
          {matches.length > 0 && (
            <div className="flex items-center gap-2 px-1">
              <span className="text-xs text-slate-500">
                {t("proxy.debug.globalSearchResultCount", {
                  current: currentIndex + 1,
                  total: matches.length,
                })}
              </span>
              <div className="flex gap-1">
                <button
                  type="button"
                  onClick={goPrev}
                  className="p-0.5 rounded hover:bg-gray-200 dark:hover:bg-gray-700 cursor-pointer"
                >
                  <ChevronUp className="w-4 h-4" />
                </button>
                <button
                  type="button"
                  onClick={goNext}
                  className="p-0.5 rounded hover:bg-gray-200 dark:hover:bg-gray-700 cursor-pointer"
                >
                  <ChevronDown className="w-4 h-4" />
                </button>
              </div>
            </div>
          )}

          {query.length >= 2 && matches.length === 0 && (
            <div className="text-sm text-slate-400 py-4 text-center">
              {t("proxy.debug.globalSearchNoResults")}
            </div>
          )}

          {/* Results grouped by section */}
          {collapseItems.length > 0 && (
            <Collapse
              size="small"
              activeKey={expandedKeys}
              onChange={(keys) =>
                setExpandedKeys(Array.isArray(keys) ? keys : [keys])
              }
              items={collapseItems}
            />
          )}
        </div>
      </Modal>
    );
  },
);

GlobalSearchModal.displayName = "GlobalSearchModal";

export default GlobalSearchModal;
