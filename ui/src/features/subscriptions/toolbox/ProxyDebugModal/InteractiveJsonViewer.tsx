import type { FieldOrigin } from "@acme/types";
import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

// ─── Exports ───

/** Read-only JSON syntax highlighter */
export function SyntaxJsonViewer({
  data,
  maxHeight,
}: {
  data: unknown;
  maxHeight?: number;
}) {
  return (
    <div
      className="!p-3 !text-xs !bg-gray-50 dark:!bg-gray-900 !rounded-md !overflow-auto !font-mono leading-5"
      style={{ maxHeight: maxHeight ?? 400 }}
    >
      <JsonNode value={data} indent={0} isLast />
    </div>
  );
}

// ─── JSON recursive renderer ───

function JsonNode({
  value,
  path = "",
  indent,
  isLast,
}: {
  value: unknown;
  path?: string;
  indent: number;
  isLast: boolean;
}) {
  if (value === null)
    return <span className="text-gray-500">null{!isLast && ","}</span>;
  if (typeof value === "boolean")
    return (
      <span className="text-purple-600 dark:text-purple-400">
        {String(value)}
        {!isLast && ","}
      </span>
    );
  if (typeof value === "number")
    return (
      <span className="text-blue-600 dark:text-blue-400">
        {value}
        {!isLast && ","}
      </span>
    );
  if (typeof value === "string")
    return (
      <span className="text-green-700 dark:text-green-400">
        &quot;{value}&quot;{!isLast && ","}
      </span>
    );

  if (Array.isArray(value)) {
    if (value.length === 0)
      return (
        <span>
          {"[]"}
          {!isLast && ","}
        </span>
      );
    return (
      <span>
        {"["}
        {value.map((item, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: JSON array order is deterministic
          <div key={`${path}[${i}]`} style={{ paddingLeft: 16 }}>
            <JsonNode
              value={item}
              path={`${path}[${i}]`}
              indent={indent + 1}
              isLast={i === value.length - 1}
            />
          </div>
        ))}
        {"]"}
        {!isLast && ","}
      </span>
    );
  }

  if (typeof value === "object" && value !== null) {
    const entries = Object.entries(value as Record<string, unknown>);
    if (entries.length === 0)
      return (
        <span>
          {"{}"}
          {!isLast && ","}
        </span>
      );

    return (
      <span>
        {"{"}
        {entries.map(([key, val], i) => {
          const childPath = path ? `${path}.${key}` : key;
          return (
            <div key={key} style={{ paddingLeft: 16 }}>
              <span className="text-gray-700 dark:text-gray-300">
                &quot;{key}&quot;
              </span>
              {": "}
              <JsonNode
                value={val}
                path={childPath}
                indent={indent + 1}
                isLast={i === entries.length - 1}
              />
            </div>
          );
        })}
        {"}"}
        {!isLast && ","}
      </span>
    );
  }

  return (
    <span>
      {String(value)}
      {!isLast && ","}
    </span>
  );
}

// ─── Constants for Provenance Table ───

const STEP_ICONS: Record<string, string> = {
  core: "🏗️",
  tls: "🔒",
  transport: "🔀",
  multiplex: "📡",
  dial: "📞",
  type: "🔧",
  unknown: "❓",
};

const TRANSFORM_COLORS: Record<string, string> = {
  direct:
    "bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-300",
  rename: "bg-sky-100 text-sky-800 dark:bg-sky-900/30 dark:text-sky-300",
  convert:
    "bg-violet-100 text-violet-800 dark:bg-violet-900/30 dark:text-violet-300",
  extract:
    "bg-amber-100 text-amber-800 dark:bg-amber-900/30 dark:text-amber-300",
  fallback:
    "bg-orange-100 text-orange-800 dark:bg-orange-900/30 dark:text-orange-300",
  container: "bg-teal-100 text-teal-800 dark:bg-teal-900/30 dark:text-teal-300",
  generated: "bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-300",
};

// ─── Provenance Table ───

interface ProvenanceRow {
  path: string;
  key: string;
  depth: number;
  value: unknown;
  origin?: FieldOrigin;
  hasChildren: boolean;
}

function buildProvenanceRows(
  data: Record<string, unknown>,
  fieldOrigins: Record<string, FieldOrigin>,
  prefix: string,
  depth: number,
): ProvenanceRow[] {
  const rows: ProvenanceRow[] = [];
  for (const [key, val] of Object.entries(data)) {
    const path = prefix ? `${prefix}.${key}` : key;
    const origin = fieldOrigins[path];
    const isObj =
      val !== null && typeof val === "object" && !Array.isArray(val);
    const isArr = Array.isArray(val);
    rows.push({
      path,
      key,
      depth,
      value: val,
      origin,
      hasChildren:
        (isObj && Object.keys(val as object).length > 0) ||
        (isArr && (val as unknown[]).length > 0),
    });
    if (isObj) {
      rows.push(
        ...buildProvenanceRows(
          val as Record<string, unknown>,
          fieldOrigins,
          path,
          depth + 1,
        ),
      );
    } else if (isArr) {
      for (let i = 0; i < (val as unknown[]).length; i++) {
        const item = (val as unknown[])[i];
        const itemPath = `${path}[${i}]`;
        const itemOrigin = fieldOrigins[itemPath];
        if (item !== null && typeof item === "object" && !Array.isArray(item)) {
          rows.push({
            path: itemPath,
            key: `[${i}]`,
            depth: depth + 1,
            value: item,
            origin: itemOrigin,
            hasChildren: Object.keys(item as object).length > 0,
          });
          rows.push(
            ...buildProvenanceRows(
              item as Record<string, unknown>,
              fieldOrigins,
              itemPath,
              depth + 2,
            ),
          );
        }
      }
    }
  }
  return rows;
}

function isDescendant(childPath: string, parentPath: string): boolean {
  return (
    childPath.startsWith(`${parentPath}.`) ||
    childPath.startsWith(`${parentPath}[`)
  );
}

function formatCellValue(
  value: unknown,
  t: (key: string, opts?: Record<string, unknown>) => string,
): string {
  if (value === null) return "null";
  if (value === undefined) return "undefined";
  if (typeof value === "boolean") return String(value);
  if (typeof value === "number") return String(value);
  if (typeof value === "string") return JSON.stringify(value);
  if (Array.isArray(value))
    return t("proxy.debug.originTableJsonArray", { count: value.length });
  if (typeof value === "object") return t("proxy.debug.originTableJsonObject");
  return String(value);
}

function getSourceDescription(
  origin: FieldOrigin | undefined,
  t: (key: string, opts?: string | Record<string, unknown>) => string,
): string {
  if (!origin) return "—";
  const icon = STEP_ICONS[origin.step] ?? "❓";
  const step = t(`proxy.debug.originStep.${origin.step}`);
  const parts: string[] = [`${icon} ${step}`];

  if (origin.transform === "container" && origin.sources) {
    parts.push(
      t("proxy.debug.originTableFromFields", {
        fields: origin.sources.join(", "),
      }) as string,
    );
  } else if (origin.sourceKey) {
    let src = `${t("proxy.debug.originSourceKey")}: .${origin.sourceKey}`;
    if (origin.sourceValue !== undefined) {
      const sv = JSON.stringify(origin.sourceValue);
      if (sv.length <= 40) src += ` = ${sv}`;
    }
    parts.push(src);
  }

  if (origin.reason && origin.reason !== "converter_internal") {
    parts.push(
      `💡 ${t(`proxy.debug.originReason.${origin.reason}`, origin.reason)}`,
    );
  }

  return parts.join(" · ");
}

const TRANSFORM_TEXT_COLORS: Record<string, string> = {
  direct: "text-green-700 dark:text-green-400",
  rename: "text-sky-700 dark:text-sky-400",
  convert: "text-violet-700 dark:text-violet-400",
  extract: "text-amber-700 dark:text-amber-400",
  fallback: "text-orange-600 dark:text-orange-400",
  container: "text-teal-600 dark:text-teal-400",
  generated: "text-red-600 dark:text-red-400",
};

/** Hierarchical provenance tree-table for field origin tracing */
export function ProvenanceTable({
  data,
  fieldOrigins,
  maxHeight,
}: {
  data: Record<string, unknown>;
  fieldOrigins: Record<string, FieldOrigin>;
  maxHeight?: number;
}) {
  const { t } = useTranslation();
  const allRows = useMemo(
    () => buildProvenanceRows(data, fieldOrigins, "", 0),
    [data, fieldOrigins],
  );
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const toggleExpand = useCallback(
    (path: string) => {
      setExpanded((prev) => {
        const next = new Set(prev);
        if (next.has(path)) {
          // Collapse: also collapse all descendants
          next.delete(path);
          for (const row of allRows) {
            if (isDescendant(row.path, path)) next.delete(row.path);
          }
        } else {
          next.add(path);
        }
        return next;
      });
    },
    [allRows],
  );

  const visibleRows = useMemo(() => {
    return allRows.filter((row) => {
      if (row.depth === 0) return true;
      // Walk up: every ancestor with children must be expanded
      for (const other of allRows) {
        if (
          other.hasChildren &&
          isDescendant(row.path, other.path) &&
          !expanded.has(other.path)
        ) {
          return false;
        }
      }
      return true;
    });
  }, [allRows, expanded]);

  return (
    <div
      className="overflow-auto border border-gray-200 dark:border-gray-700 rounded-md"
      style={{ maxHeight: maxHeight ?? 500 }}
    >
      <table className="w-full text-xs border-collapse">
        <thead className="sticky top-0 z-10">
          <tr className="bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300">
            <th className="text-left px-2 py-1.5 font-semibold border-b border-gray-200 dark:border-gray-700 whitespace-nowrap">
              {t("proxy.debug.originTablePath")}
            </th>
            <th className="text-left px-2 py-1.5 font-semibold border-b border-gray-200 dark:border-gray-700 whitespace-nowrap">
              {t("proxy.debug.originTableValue")}
            </th>
            <th className="text-left px-2 py-1.5 font-semibold border-b border-gray-200 dark:border-gray-700 whitespace-nowrap">
              {t("proxy.debug.originTableTransform")}
            </th>
            <th className="text-left px-2 py-1.5 font-semibold border-b border-gray-200 dark:border-gray-700">
              {t("proxy.debug.originTableSource")}
            </th>
          </tr>
        </thead>
        <tbody>
          {visibleRows.map((row) => {
            const isExpanded = expanded.has(row.path);
            const transformColor = row.origin
              ? (TRANSFORM_TEXT_COLORS[row.origin.transform] ?? "")
              : "";
            const transformBgColor = row.origin
              ? (TRANSFORM_COLORS[row.origin.transform] ?? "")
              : "";

            return (
              <tr
                key={row.path}
                className="border-b border-gray-100 dark:border-gray-800 hover:bg-gray-50 dark:hover:bg-gray-800/50"
              >
                <td className="px-2 py-1 font-mono whitespace-nowrap align-top">
                  <span style={{ paddingLeft: row.depth * 16 }}>
                    {row.hasChildren ? (
                      <button
                        type="button"
                        className="inline-flex items-center gap-0.5 cursor-pointer"
                        onClick={() => toggleExpand(row.path)}
                      >
                        <span className="w-3.5 text-gray-400 text-[10px] select-none">
                          {isExpanded ? "▼" : "▶"}
                        </span>
                        <span
                          className={
                            transformColor || "text-gray-700 dark:text-gray-300"
                          }
                        >
                          {row.key}
                        </span>
                      </button>
                    ) : (
                      <span className="inline-flex items-center gap-0.5">
                        <span className="w-3.5" />
                        <span
                          className={
                            transformColor || "text-gray-700 dark:text-gray-300"
                          }
                        >
                          {row.key}
                        </span>
                      </span>
                    )}
                  </span>
                </td>
                <td className="px-2 py-1 font-mono max-w-[200px] truncate align-top">
                  <span
                    className={
                      typeof row.value === "string"
                        ? "text-green-700 dark:text-green-400"
                        : typeof row.value === "number"
                          ? "text-blue-700 dark:text-blue-400"
                          : typeof row.value === "boolean"
                            ? "text-purple-700 dark:text-purple-400"
                            : row.value === null
                              ? "text-gray-400"
                              : "text-gray-500 dark:text-gray-400"
                    }
                    title={
                      typeof row.value === "string" ||
                      typeof row.value === "number" ||
                      typeof row.value === "boolean"
                        ? String(row.value)
                        : undefined
                    }
                  >
                    {formatCellValue(row.value, t)}
                  </span>
                </td>
                <td className="px-2 py-1 whitespace-nowrap align-top">
                  {row.origin && (
                    <span
                      className={`px-1.5 py-0.5 rounded text-[10px] font-semibold ${transformBgColor}`}
                    >
                      {t(`proxy.debug.originTransform.${row.origin.transform}`)}
                    </span>
                  )}
                </td>
                <td className="px-2 py-1 text-gray-600 dark:text-gray-400 align-top max-w-[300px]">
                  <span className="break-words">
                    {getSourceDescription(row.origin, t)}
                  </span>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
