import {
  CheckCircleOutlined,
  CloseCircleOutlined,
  ExclamationCircleOutlined,
  InfoCircleOutlined,
  MinusCircleOutlined,
} from "@acme/components";
import type { ProxyDebugFormat, ProxyNodeTraceStep } from "@acme/types";
import { useTranslation } from "react-i18next";

import {
  ConvertTraceContent,
  EnrichTraceContent,
  FilterTraceContent,
  GroupAssignTraceContent,
  MergeTraceContent,
  OutputTraceContent,
  ParseTraceContent,
  SourceTraceContent,
} from "./NodeTraceContent";

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
export const TraceStepsContent = ({
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
