import {
  AimOutlined,
  ArrowLeftOutlined,
  AutoComplete,
  Button,
  CheckCircleOutlined,
  CloseCircleOutlined,
  Collapse,
  Descriptions,
  Empty,
  ExclamationCircleOutlined,
  InfoCircleOutlined,
  MinusCircleOutlined,
  Modal,
  Spin,
  Tag,
} from "@acme/components";
import type { ProxyDebugFormat, ProxyNodeTraceStep } from "@acme/types";
import {
  forwardRef,
  useCallback,
  useImperativeHandle,
  useMemo,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { proxyApi } from "@/generated/rust-api";
import { useIsMobile } from "@/hooks";
import { ProvenanceTable, SyntaxJsonViewer } from "./InteractiveJsonViewer";

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

/** Try to render as syntax-highlighted JSON, fall back to plain CodeBlock */
const OutputJsonBlock = ({ content }: { content: string }) => {
  let parsed: unknown;
  try {
    parsed = JSON.parse(content);
  } catch {
    return <CodeBlock content={content} maxHeight={400} />;
  }
  return <SyntaxJsonViewer data={parsed} maxHeight={400} />;
};

/** 追踪步骤: 来源 */
const SourceTraceContent = ({
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
        defaultActiveKey={data.rawUrl ? ["rawUrl"] : []}
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
                    <SyntaxJsonViewer data={data.rawData} maxHeight={300} />
                  ),
                },
              ]),
        ]}
      />
    </div>
  );
};

/** 追踪步骤: 解析 */
const ParseTraceContent = ({
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
            <SyntaxJsonViewer data={step.data.clashProxy} maxHeight={300} />
          ),
        },
      ]}
    />
  );
};

/** 追踪步骤: 过滤 */
const FilterTraceContent = ({
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
const EnrichTraceContent = ({
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
const MergeTraceContent = ({
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
const GroupAssignTraceContent = ({
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
const ConvertTraceContent = ({
  step,
}: {
  step: Extract<ProxyNodeTraceStep, { type: "convert" }>;
}) => {
  const { t } = useTranslation();
  const lostFields = step.data.lostFields ?? [];
  const ignoredFields = step.data.ignoredFields ?? [];
  const fieldOrigins = (step.data.fieldOrigins ?? {}) as Record<
    string,
    import("@acme/types").FieldOrigin
  >;

  return (
    <div className="flex flex-col gap-2">
      {lostFields.length > 0 && (
        <div className="flex flex-wrap items-center gap-1 px-3 py-2 rounded-md bg-amber-50 dark:bg-amber-900/20 text-amber-700 dark:text-amber-300 text-xs">
          <ExclamationCircleOutlined className="shrink-0" />
          <span className="font-semibold mr-1">
            {t("proxy.debug.lostFieldsLabel")}
          </span>
          {lostFields.map((field) => (
            <Tag key={field} color="gold" className="!text-xs">
              {field}
            </Tag>
          ))}
        </div>
      )}
      {ignoredFields.length > 0 && (
        <div className="flex flex-wrap items-center gap-1 px-3 py-2 rounded-md bg-blue-50 dark:bg-blue-900/20 text-blue-700 dark:text-blue-300 text-xs">
          <InfoCircleOutlined className="shrink-0" />
          <span className="font-semibold mr-1">
            {t("proxy.debug.ignoredFieldsLabel")}
          </span>
          {ignoredFields.map((field) => (
            <Tag key={field} color="blue" className="!text-xs">
              {field}
            </Tag>
          ))}
        </div>
      )}
      <Collapse
        size="small"
        defaultActiveKey={["outbound"]}
        items={[
          {
            key: "outbound",
            label: t("proxy.debug.traceSingboxOutbound"),
            children: (
              <SyntaxJsonViewer
                data={step.data.singboxOutbound}
                maxHeight={400}
              />
            ),
          },
          ...(Object.keys(fieldOrigins).length > 0
            ? [
                {
                  key: "provenance",
                  label: t("proxy.debug.traceProvenanceTable"),
                  children: (
                    <ProvenanceTable
                      data={
                        step.data.singboxOutbound as Record<string, unknown>
                      }
                      fieldOrigins={fieldOrigins}
                      maxHeight={500}
                    />
                  ),
                },
              ]
            : []),
        ]}
      />
    </div>
  );
};

/** 追踪步骤: 最终输出 */
const OutputTraceContent = ({
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
          children: <OutputJsonBlock content={step.data.configFragment} />,
        },
      ]}
    />
  );
};

/** 所有可能的追踪步骤类型，按逻辑顺序 */
const ALL_TRACE_STEP_TYPES = [
  "source",
  "parse",
  "filter",
  "enrich",
  "merge",
  "group-assign",
  "convert",
  "output",
] as const;

/** 追踪步骤内容渲染 */
const TraceStepsContent = ({
  data,
  format,
}: {
  data: { nodeName: string; steps: ProxyNodeTraceStep[] };
  format: ProxyDebugFormat;
}) => {
  const { t } = useTranslation();

  const renderStepContent = (step: ProxyNodeTraceStep) => {
    switch (step.type) {
      case "source":
        return <SourceTraceContent step={step} />;
      case "parse":
        return <ParseTraceContent step={step} />;
      case "filter":
        return <FilterTraceContent step={step} />;
      case "enrich":
        return <EnrichTraceContent step={step} />;
      case "merge":
        return <MergeTraceContent step={step} />;
      case "group-assign":
        return <GroupAssignTraceContent step={step} />;
      case "convert":
        return <ConvertTraceContent step={step} />;
      case "output":
        return <OutputTraceContent step={step} />;
    }
  };

  const existingStepTypes = new Set(data.steps.map((s) => s.type));
  const filterStep = data.steps.find((s) => s.type === "filter");
  const isFiltered = filterStep?.type === "filter" && !filterStep.data.passed;

  const displaySteps = ALL_TRACE_STEP_TYPES.filter((stepType) => {
    if (
      stepType === "convert" &&
      !format.startsWith("sing-box")
    ) {
      return false;
    }
    return true;
  }).map((stepType) => {
    const actualStep = data.steps.find((s) => s.type === stepType);
    const isSkipped = !existingStepTypes.has(stepType) && isFiltered;
    return { stepType, actualStep, isSkipped };
  });

  const getStepLabel = (
    stepType: (typeof ALL_TRACE_STEP_TYPES)[number],
  ): string => {
    const labels: Record<string, string> = {
      source: t("proxy.debug.traceSource"),
      parse: t("proxy.debug.traceParse"),
      filter: t("proxy.debug.traceFilter"),
      enrich: t("proxy.debug.traceEnrich"),
      merge: t("proxy.debug.traceMerge"),
      "group-assign": t("proxy.debug.traceGroupAssign"),
      convert: t("proxy.debug.traceConvert"),
      output: t("proxy.debug.traceOutput"),
    };
    return labels[stepType] || stepType;
  };

  return (
    <div className="space-y-0">
      {displaySteps.map(({ stepType, actualStep, isSkipped }, index) => {
        const isLast = index === displaySteps.length - 1;
        const hasConvertWarning =
          stepType === "convert" &&
          actualStep?.type === "convert" &&
          (actualStep.data.lostFields?.length ?? 0) > 0;
        const hasConvertInfo =
          stepType === "convert" &&
          actualStep?.type === "convert" &&
          !hasConvertWarning &&
          (actualStep.data.ignoredFields?.length ?? 0) > 0;
        const icon = isSkipped ? (
          <MinusCircleOutlined className="text-gray-400" />
        ) : actualStep ? (
          stepType === "filter" && isFiltered ? (
            <CloseCircleOutlined className="text-red-500" />
          ) : hasConvertWarning ? (
            <ExclamationCircleOutlined />
          ) : hasConvertInfo ? (
            <InfoCircleOutlined />
          ) : (
            <CheckCircleOutlined />
          )
        ) : undefined;
        const color = isSkipped
          ? "#9ca3af"
          : actualStep
            ? stepType === "filter" && isFiltered
              ? "#ef4444"
              : hasConvertWarning
                ? "#f59e0b"
                : hasConvertInfo
                  ? "#3b82f6"
                  : "#22c55e"
            : "#9ca3af";

        return (
          <div key={stepType} className="flex gap-3">
            <div className="flex flex-col items-center">
              <div
                className="flex items-center justify-center w-6 h-6 text-lg shrink-0"
                style={{ color }}
              >
                {icon}
              </div>
              {!isLast && (
                <div className="w-0.5 flex-1 bg-gray-200 dark:bg-gray-700 my-1" />
              )}
            </div>
            <div className="flex-1 pb-4">
              <div
                className={`font-semibold text-sm ${isSkipped ? "text-slate-500" : ""}`}
              >
                {getStepLabel(stepType)}
              </div>
              {isSkipped ? (
                <div className="text-xs text-slate-500 mt-2 mb-4">
                  {t("proxy.debug.traceSkipped")}
                </div>
              ) : actualStep ? (
                <div className="mt-2 mb-4">{renderStepContent(actualStep)}</div>
              ) : null}
            </div>
          </div>
        );
      })}
    </div>
  );
};

// ============================================
// NodeTraceModal
// ============================================

export interface NodeTraceModalRef {
  open: (nodeName?: string) => void;
}

interface NodeTraceModalProps {
  subscribeId: string;
  format: ProxyDebugFormat;
  allNodeNames: { name: string; filtered: boolean }[];
  nodeWarnings?: Set<string>;
  nodeIgnored?: Set<string>;
}

const NodeTraceModal = forwardRef<NodeTraceModalRef, NodeTraceModalProps>(
  ({ subscribeId, format, allNodeNames, nodeWarnings, nodeIgnored }, ref) => {
    const { t } = useTranslation();
    const isMobile = useIsMobile();
    const [visible, setVisible] = useState(false);
    const [tracingNodeName, setTracingNodeName] = useState<string | null>(null);
    const [searchValue, setSearchValue] = useState("");

    useImperativeHandle(ref, () => ({
      open: (nodeName?: string) => {
        if (nodeName) {
          setTracingNodeName(nodeName);
          setSearchValue(nodeName);
        } else {
          setTracingNodeName(null);
          setSearchValue("");
        }
        setVisible(true);
      },
    }));

    const { data, isLoading, error } = proxyApi.traceNode.useQuery(
      tracingNodeName
        ? { id: subscribeId, format, nodeName: tracingNodeName }
        : (undefined as unknown as {
            id: string;
            format: string;
            nodeName: string;
          }),
      { enabled: !!tracingNodeName },
    );

    const handleTraceNode = useCallback((nodeName: string) => {
      setTracingNodeName(nodeName);
      setSearchValue(nodeName);
    }, []);

    const handleClose = () => {
      setVisible(false);
    };

    const autoCompleteOptions = useMemo(() => {
      const query = searchValue.toLowerCase();
      return allNodeNames
        .filter((n) => !query || n.name.toLowerCase().includes(query))
        .slice(0, 50)
        .map((n) => ({
          value: n.name,
          label: (
            <div className="flex items-center justify-between">
              <span
                className={`text-xs truncate flex-1 ${n.filtered ? "text-slate-500 line-through" : ""}`}
              >
                {n.name}
              </span>
              {n.filtered && (
                <Tag color="orange" className="!text-xs ml-1 shrink-0">
                  {t("proxy.debug.traceFilteredLabel")}
                </Tag>
              )}
            </div>
          ),
        }));
    }, [allNodeNames, searchValue, t]);

    const filterStep = data?.steps?.find((s) => s.type === "filter") as
      | Extract<ProxyNodeTraceStep, { type: "filter" }>
      | undefined;
    const isFiltered = filterStep?.type === "filter" && !filterStep.data.passed;

    return (
      <Modal
        title={
          <div className="flex min-w-0 flex-wrap items-center gap-3">
            <Button
              variant="text"
              size="small"
              icon={<ArrowLeftOutlined />}
              onClick={handleClose}
            />
            <AimOutlined className="text-blue-500" />
            <span>{t("proxy.debug.traceTitle")}</span>
            {tracingNodeName && (
              <Tag
                color={
                  nodeWarnings?.has(tracingNodeName)
                    ? "gold"
                    : nodeIgnored?.has(tracingNodeName)
                      ? "blue"
                      : "green"
                }
              >
                {tracingNodeName}
              </Tag>
            )}
            {isFiltered && (
              <Tag color="orange">{t("proxy.debug.traceFilteredLabel")}</Tag>
            )}
          </div>
        }
        open={visible}
        onCancel={handleClose}
        footer={null}
        size={isMobile ? "full" : "almost-full"}
        destroyOnClose={false}
      >
        {/* 搜索栏 */}
        <div className="flex items-center gap-3 mb-4 flex-wrap">
          <AutoComplete
            value={searchValue}
            options={autoCompleteOptions}
            onSearch={(value: string) => {
              setSearchValue(value);
              if (!value) {
                setTracingNodeName(null);
              }
            }}
            onSelect={(value: string) => handleTraceNode(value)}
            placeholder={t("proxy.debug.traceSearchPlaceholder")}
            className="flex-1 min-w-[200px] max-w-[500px]"
            allowClear
          />
          <span className="text-slate-500 text-xs">
            {t("proxy.debug.traceNodeList")}: {allNodeNames.length}
          </span>
        </div>

        {/* 内容区 */}
        {!tracingNodeName && (
          <div className="text-center py-12">
            <Empty description={t("proxy.debug.traceSelectNode")} />
          </div>
        )}

        {tracingNodeName && isLoading && (
          <div className="flex items-center justify-center py-12">
            <Spin />
            <span className="text-slate-500 ml-2">
              {t("proxy.debug.traceLoading")}
            </span>
          </div>
        )}

        {tracingNodeName && error && (
          <div className="mb-4 p-3 rounded-lg border border-red-200 bg-red-50 dark:border-red-800 dark:bg-red-900/20">
            <div className="font-semibold text-red-600 dark:text-red-400">
              {t("proxy.debug.error")}
            </div>
            <div className="text-sm text-red-500 dark:text-red-400 mt-1">
              {error.message}
            </div>
          </div>
        )}

        {tracingNodeName && data && data.steps.length === 0 && !isLoading && (
          <Empty description={t("proxy.debug.traceNodeNotFound")} />
        )}

        {tracingNodeName && data && data.steps.length > 0 && (
          <TraceStepsContent
            data={data as { nodeName: string; steps: ProxyNodeTraceStep[] }}
            format={format}
          />
        )}
      </Modal>
    );
  },
);

NodeTraceModal.displayName = "NodeTraceModal";

export default NodeTraceModal;
