import {
  CheckCircleOutlined,
  CloseCircleOutlined,
  Collapse,
  Descriptions,
  Tag,
} from "@acme/components";
import type { ProxyNodeTraceStep } from "@acme/types";
import { useTranslation } from "react-i18next";

/** 渲染 JSON 或 YAML 格式的代码块 */
const CodeBlock = ({
  content,
  maxHeight,
}: {
  content: string;
  maxHeight?: number;
}) => (
  <pre
    className="!m-0 !p-3 !text-xs !bg-gray-50 dark:!bg-gray-900 !rounded-md !overflow-auto !whitespace-pre-wrap !break-all !font-mono"
    style={{ maxHeight: maxHeight ?? 400 }}
  >
    {content}
  </pre>
);

/** 追踪步骤: 来源 */
export const SourceTraceContent = ({
  step,
}: {
  step: Extract<ProxyNodeTraceStep, { type: "source" }>;
}) => {
  const { t } = useTranslation();
  const { data } = step;

  return (
    <div className="flex flex-col gap-2">
      <Descriptions
        size="small"
        column={2}
        bordered
        items={[
          {
            label: t("proxy.debug.traceSourceIndex"),
            children: (
              <Tag color={data.sourceIndex === 0 ? "default" : "blue"}>
                {data.sourceIndex === 0
                  ? t("proxy.debug.traceManual")
                  : `#${data.sourceIndex}`}
              </Tag>
            ),
          },
          {
            label: t("proxy.debug.traceSourceUrl"),
            children: (
              <span className="text-xs break-all">{data.sourceUrl}</span>
            ),
          },
          {
            label: t("proxy.debug.traceSourceFormat"),
            children: <Tag color="processing">{data.format}</Tag>,
          },
        ]}
      />
      <Collapse
        size="small"
        items={[
          ...(data.rawUrl
            ? [
                {
                  key: "rawUrl",
                  label: t("proxy.debug.traceRawUrl"),
                  children: <CodeBlock content={data.rawUrl} maxHeight={300} />,
                },
              ]
            : [
                {
                  key: "raw",
                  label: t("proxy.debug.traceRawData"),
                  children: (
                    <CodeBlock
                      content={JSON.stringify(data.rawData, null, 2)}
                      maxHeight={300}
                    />
                  ),
                },
              ]),
        ]}
      />
    </div>
  );
};

/** 追踪步骤: 解析 */
export const ParseTraceContent = ({
  step,
}: {
  step: Extract<ProxyNodeTraceStep, { type: "parse" }>;
}) => {
  const { t } = useTranslation();

  return (
    <Collapse
      size="small"
      defaultActiveKey={["clash"]}
      items={[
        {
          key: "clash",
          label: t("proxy.debug.traceClashProxy"),
          children: (
            <CodeBlock
              content={JSON.stringify(step.data.clashProxy, null, 2)}
              maxHeight={300}
            />
          ),
        },
      ]}
    />
  );
};

/** 追踪步骤: 过滤 */
export const FilterTraceContent = ({
  step,
}: {
  step: Extract<ProxyNodeTraceStep, { type: "filter" }>;
}) => {
  const { t } = useTranslation();
  const { data } = step;

  return (
    <div className="flex flex-col gap-2">
      <Descriptions
        size="small"
        column={2}
        bordered
        items={[
          {
            label: t("proxy.debug.traceFilter"),
            children: data.passed ? (
              <Tag icon={<CheckCircleOutlined />} color="success">
                {t("proxy.debug.traceFilterPassed")}
              </Tag>
            ) : (
              <Tag icon={<CloseCircleOutlined />} color="error">
                {t("proxy.debug.traceFilterBlocked")}
              </Tag>
            ),
          },
          ...(data.matchedRule
            ? [
                {
                  label: t("proxy.debug.traceMatchedRule"),
                  children: <Tag color="orange">{data.matchedRule}</Tag>,
                },
              ]
            : []),
        ]}
      />
      {data.filtersApplied.length > 0 && (
        <Collapse
          size="small"
          items={[
            {
              key: "rules",
              label: (
                <span>
                  {t("proxy.debug.traceFilterRules")}{" "}
                  <Tag>{data.filtersApplied.length}</Tag>
                </span>
              ),
              children: (
                <div className="flex flex-wrap gap-1">
                  {data.filtersApplied.map((f: string) => (
                    <Tag
                      key={f}
                      color={f === data.matchedRule ? "orange" : "default"}
                    >
                      {f}
                      {f === data.matchedRule && " ✓"}
                    </Tag>
                  ))}
                </div>
              ),
            },
          ]}
        />
      )}
    </div>
  );
};

/** 追踪步骤: 名称富化 */
export const EnrichTraceContent = ({
  step,
}: {
  step: Extract<ProxyNodeTraceStep, { type: "enrich" }>;
}) => {
  const { t } = useTranslation();
  const { data } = step;
  const nameChanged = data.originalName !== data.enrichedName;

  return (
    <Descriptions
      size="small"
      column={1}
      bordered
      items={[
        {
          label: t("proxy.debug.traceOriginalName"),
          children: (
            <span className="text-xs font-mono">{data.originalName}</span>
          ),
        },
        {
          label: t("proxy.debug.traceEnrichedName"),
          children: (
            <span className="text-xs font-mono">
              {data.enrichedName}
              {nameChanged && (
                <Tag color="green" className="ml-2">
                  ✨
                </Tag>
              )}
            </span>
          ),
        },
      ]}
    />
  );
};

/** 追踪步骤: 合并 */
export const MergeTraceContent = ({
  step,
}: {
  step: Extract<ProxyNodeTraceStep, { type: "merge" }>;
}) => {
  const { t } = useTranslation();
  const { data } = step;

  return (
    <Descriptions
      size="small"
      column={2}
      bordered
      items={[
        {
          label: t("proxy.debug.tracePosition"),
          children: (
            <Tag color="blue">
              #{data.positionInFinalList} / {data.totalNodes}
            </Tag>
          ),
        },
      ]}
    />
  );
};

/** 追踪步骤: 分组分配 */
export const GroupAssignTraceContent = ({
  step,
}: {
  step: Extract<ProxyNodeTraceStep, { type: "group-assign" }>;
}) => {
  const { t } = useTranslation();
  const { data } = step;

  if (data.assignedGroups.length === 0) {
    return (
      <span className="text-slate-500">
        {t("proxy.debug.traceNoGroupAssigned")}
      </span>
    );
  }

  return (
    <div className="flex flex-wrap gap-1">
      {data.assignedGroups.map((g) => (
        <Tag key={g.name} color="purple">
          {g.name} <span className="text-slate-500 text-xs">({g.type})</span>
        </Tag>
      ))}
    </div>
  );
};

/** 追踪步骤: 格式转换 */
export const ConvertTraceContent = ({
  step,
}: {
  step: Extract<ProxyNodeTraceStep, { type: "convert" }>;
}) => {
  const { t } = useTranslation();

  return (
    <Collapse
      size="small"
      defaultActiveKey={["outbound"]}
      items={[
        {
          key: "outbound",
          label: t("proxy.debug.traceSingboxOutbound"),
          children: (
            <CodeBlock
              content={JSON.stringify(step.data.singboxOutbound, null, 2)}
              maxHeight={400}
            />
          ),
        },
      ]}
    />
  );
};

/** 追踪步骤: 最终输出 */
export const OutputTraceContent = ({
  step,
}: {
  step: Extract<ProxyNodeTraceStep, { type: "output" }>;
}) => {
  const { t } = useTranslation();

  return (
    <Collapse
      size="small"
      defaultActiveKey={["fragment"]}
      items={[
        {
          key: "fragment",
          label: t("proxy.debug.traceConfigFragment"),
          children: (
            <CodeBlock content={step.data.configFragment} maxHeight={400} />
          ),
        },
      ]}
    />
  );
};

/** 所有可能的追踪步骤类型，按逻辑顺序 */
