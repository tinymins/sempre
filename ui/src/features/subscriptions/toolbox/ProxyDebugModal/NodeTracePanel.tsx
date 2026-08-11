import {
  AimOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
  Empty,
  MinusCircleOutlined,
  Spin,
  Tag,
} from "@acme/components";
import type { ProxyDebugFormat, ProxyNodeTraceStep } from "@acme/types";
import { useTranslation } from "react-i18next";
import { proxyApi } from "@/generated/rust-api";

import {
  ConvertTraceContent,
  EnrichTraceContent,
  FilterTraceContent,
  GroupAssignTraceContent,
  MergeTraceContent,
  OutputTraceContent,
  ParseTraceContent,
  SourceTraceContent,
} from "./NodeTracePanelContent";

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

interface NodeTracePanelProps {
  subscribeId: string;
  format: ProxyDebugFormat;
  nodeName: string;
}

const NodeTracePanel = ({
  subscribeId,
  format,
  nodeName,
}: NodeTracePanelProps) => {
  const { t } = useTranslation();

  const { data, isLoading, error } = proxyApi.traceNode.useQuery(
    { id: subscribeId, format, nodeName },
    { enabled: !!nodeName },
  );

  /** 获取步骤的显示标签 */
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

  /** 渲染步骤内容 */
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

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-8">
        <Spin />
        <span className="text-slate-500 ml-2">
          {t("proxy.debug.traceLoading")}
        </span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="mb-4 p-3 rounded-lg border border-red-200 bg-red-50 dark:border-red-800 dark:bg-red-900/20">
        <div className="font-semibold text-red-600 dark:text-red-400">
          {t("proxy.debug.error")}
        </div>
        <div className="text-sm text-red-500 dark:text-red-400 mt-1">
          {error.message}
        </div>
      </div>
    );
  }

  if (!data || data.steps.length === 0) {
    return <Empty description={t("proxy.debug.traceNodeNotFound")} />;
  }

  // 找出存在的步骤类型
  const existingStepTypes = new Set(data.steps.map((s) => s.type));

  // 检查节点是否被过滤
  const filterStep = data.steps.find((s) => s.type === "filter") as
    | Extract<ProxyNodeTraceStep, { type: "filter" }>
    | undefined;
  const isFiltered = filterStep?.type === "filter" && !filterStep.data.passed;

  // 构建显示步骤列表：包含已执行的和被跳过的步骤
  const displaySteps = ALL_TRACE_STEP_TYPES.filter((stepType) => {
    // sing-box 相关格式才显示 convert 步骤
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

  return (
    <div className="border-t border-gray-200 dark:border-gray-700 pt-4 mt-4">
      <div className="flex items-center gap-2 mb-4">
        <AimOutlined className="text-blue-500" />
        <span className="font-semibold">{t("proxy.debug.traceTitle")}</span>
        <Tag color="blue">{data.nodeName}</Tag>
        {isFiltered && (
          <Tag color="orange">{t("proxy.debug.traceFilteredLabel")}</Tag>
        )}
      </div>

      <div className="space-y-0">
        {displaySteps.map(({ stepType, actualStep, isSkipped }, index) => {
          const isLast = index === displaySteps.length - 1;
          const icon = isSkipped ? (
            <MinusCircleOutlined className="text-gray-400" />
          ) : actualStep ? (
            stepType === "filter" && isFiltered ? (
              <CloseCircleOutlined className="text-red-500" />
            ) : (
              <CheckCircleOutlined />
            )
          ) : undefined;
          const color = isSkipped
            ? "#9ca3af"
            : actualStep
              ? stepType === "filter" && isFiltered
                ? "#ef4444"
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
                  <div className="mt-2 mb-4">
                    {renderStepContent(actualStep as ProxyNodeTraceStep)}
                  </div>
                ) : null}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

export default NodeTracePanel;
