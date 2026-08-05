import {
  AimOutlined,
  BugOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
  LoadingOutlined,
  Modal,
  SearchOutlined,
  ShieldCheckOutlined,
  Tag,
} from "@acme/components";
import type { ProxyDebugFormat, ProxyDebugStep } from "@acme/types";
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
import { proxyApi } from "@/generated/rust-api";
import {
  ConfigStepContent,
  DoneStepContent,
  ManualServersStepContent,
  MergeStepContent,
  OutputStepContent,
  SourceResultStepContent,
  SourceStartStepContent,
  ValidateStepContent,
} from "./DebugStepContent";
import type { GlobalSearchModalRef } from "./GlobalSearchModal";
import GlobalSearchModal from "./GlobalSearchModal";
import type { NodeTraceModalRef } from "./NodeTraceModal";
import NodeTraceModal from "./NodeTraceModal";
import { RuleSetsStepContent } from "./RuleSetsStepContent";

export interface ProxyDebugModalRef {
  open: (subscribeId: string, format: ProxyDebugFormat) => void;
}

const ProxyDebugModal = forwardRef<ProxyDebugModalRef>((_, ref) => {
  const { t } = useTranslation();
  const [visible, setVisible] = useState(false);
  const [subscribeId, setSubscribeId] = useState<string | null>(null);
  const [format, setFormat] = useState<ProxyDebugFormat>("clash");
  const [steps, setSteps] = useState<ProxyDebugStep[]>([]);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const traceModalRef = useRef<NodeTraceModalRef>(null);
  const globalSearchRef = useRef<GlobalSearchModalRef>(null);

  const scrollToBottom = useCallback(() => {
    setTimeout(() => {
      bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }, 100);
  }, []);

  useImperativeHandle(ref, () => ({
    open: (id: string, fmt: ProxyDebugFormat) => {
      setSubscribeId(id);
      setFormat(fmt);
      setSteps([]);
      setDone(false);
      setError(null);
      setVisible(true);
    },
  }));

  // Subscribe to debug stream via SSE
  useEffect(() => {
    if (!subscribeId || !visible) return;

    const controller = new AbortController();
    proxyApi.debugSubscription
      .stream(
        { id: subscribeId, format },
        (chunk: unknown) => {
          const step = chunk as ProxyDebugStep;
          setSteps((prev) => {
            if (step.type === "source-result") {
              const filtered = prev.filter(
                (s) =>
                  !(
                    s.type === "source-start" &&
                    s.data.sourceIndex === step.data.sourceIndex
                  ),
              );
              return [...filtered, step];
            }
            return [...prev, step];
          });
          if (step.type === "done") {
            setDone(true);
          }
          scrollToBottom();
        },
        controller.signal,
      )
      .catch((err) => {
        if (err.name !== "AbortError") {
          setError(err.message);
        }
      });

    return () => controller.abort();
  }, [subscribeId, format, visible, scrollToBottom]);

  // 收集所有节点名称（有效节点 + 被过滤节点）
  const allNodeNames = useMemo(() => {
    if (!done) return [];
    const names: { name: string; filtered: boolean }[] = [];

    // 从手动服务器步骤获取
    for (const step of steps) {
      if (step.type === "manual-servers") {
        for (const node of step.data.nodes) {
          names.push({ name: node.name, filtered: false });
        }
      }
    }

    // 从远程订阅源结果获取
    for (const step of steps) {
      if (step.type === "source-result") {
        for (const node of step.data.nodesAfterFilter) {
          names.push({ name: node.name, filtered: false });
        }
        for (const fn of step.data.filteredNodes) {
          names.push({ name: fn.node.name, filtered: true });
        }
      }
    }

    return names;
  }, [steps, done]);

  /** 从 merge 步骤提取节点状态集合 */
  const { nodeWarningSet, nodeIgnoredSet } = useMemo(() => {
    const mergeStep = steps.find((s) => s.type === "merge");
    if (!mergeStep || mergeStep.type !== "merge") {
      return {
        nodeWarningSet: new Set<string>(),
        nodeIgnoredSet: new Set<string>(),
      };
    }
    return {
      nodeWarningSet: new Set(mergeStep.data.nodeWarnings ?? []),
      nodeIgnoredSet: new Set(mergeStep.data.nodeIgnored ?? []),
    };
  }, [steps]);

  /** 处理追踪节点 */
  const handleTraceNode = useCallback((nodeName: string) => {
    traceModalRef.current?.open(nodeName);
  }, []);

  const handleClose = () => {
    setVisible(false);
    // Delay clearing subscribeId to allow cleanup
    setTimeout(() => {
      setSubscribeId(null);
      setSteps([]);
      setDone(false);
      setError(null);
    }, 300);
  };

  /** Map step type to display label */
  const getStepLabel = (step: ProxyDebugStep): string => {
    switch (step.type) {
      case "config":
        return t("proxy.debug.configParsing");
      case "manual-servers":
        return t("proxy.debug.manualServers");
      case "source-start":
        return `${t("proxy.debug.remoteSources")} #${step.data.sourceIndex}`;
      case "source-result":
        return `${t("proxy.debug.remoteSources")} #${step.data.sourceIndex}`;
      case "merge":
        return t("proxy.debug.nodeMerge");
      case "output":
        return t("proxy.debug.configBuild");
      case "rule-sets":
        return t("proxy.debug.ruleSets");
      case "validate":
        return t("proxy.debug.validate");
      case "done":
        return t("proxy.debug.complete");
      default:
        return (step as { type: string }).type;
    }
  };

  /** Get step status icon */
  const getStepStatus = (
    step: ProxyDebugStep,
  ): "process" | "finish" | "error" => {
    if (step.type === "source-start") return "process";
    if (step.type === "source-result" && step.data.error) return "error";
    if (
      step.type === "validate" &&
      !step.data.skipped &&
      step.data.valid === false
    )
      return "error";
    if (step.type === "rule-sets" && step.data.errorCount > 0) return "error";
    return "finish";
  };

  const getStepIcon = (step: ProxyDebugStep) => {
    if (step.type === "validate") return <ShieldCheckOutlined />;
    const status = getStepStatus(step);
    if (status === "process") return <LoadingOutlined />;
    if (status === "error") return <CloseCircleOutlined />;
    return <CheckCircleOutlined />;
  };

  const formatLabel: Record<ProxyDebugFormat, string> = {
    clash: "Clash",
    "clash-meta": "Clash Meta",
    "sing-box": "Sing-box",
    "sing-box-windows": "Sing-box Windows",
    "sing-box-macos": "Sing-box macOS",
    "sing-box-v12": "Sing-box v1.12",
    "sing-box-v12-windows": "Sing-box v1.12 Windows",
    "sing-box-v12-macos": "Sing-box v1.12 macOS",
    "sing-box-v13": "Sing-box v1.13",
    "sing-box-v13-windows": "Sing-box v1.13 Windows",
    "sing-box-v13-macos": "Sing-box v1.13 macOS",
  };

  /** Render step content */
  const renderStepContent = (step: ProxyDebugStep) => {
    switch (step.type) {
      case "config":
        return <ConfigStepContent step={step} />;
      case "manual-servers":
        return (
          <ManualServersStepContent
            step={step}
            onTraceNode={done ? handleTraceNode : undefined}
          />
        );
      case "source-start":
        return <SourceStartStepContent step={step} />;
      case "source-result":
        return (
          <SourceResultStepContent
            step={step}
            onTraceNode={done ? handleTraceNode : undefined}
          />
        );
      case "merge":
        return (
          <MergeStepContent
            step={step}
            onTraceNode={done ? handleTraceNode : undefined}
          />
        );
      case "output":
        return <OutputStepContent step={step} />;
      case "rule-sets":
        return <RuleSetsStepContent step={step} />;
      case "validate":
        return <ValidateStepContent step={step} />;
      case "done":
        return <DoneStepContent step={step} />;
    }
  };

  return (
    <Modal
      title={
        <div className="flex gap-2 items-center">
          <BugOutlined />
          <span>{t("proxy.debug.title")}</span>
          <Tag color="processing">{formatLabel[format]}</Tag>
        </div>
      }
      open={visible}
      onCancel={handleClose}
      footer={null}
      size="full"
      destroyOnClose
    >
      {error && (
        <div className="mb-4 p-3 rounded-lg border border-red-200 bg-red-50 dark:border-red-800 dark:bg-red-900/20">
          <div className="font-semibold text-red-600 dark:text-red-400">
            {t("proxy.debug.error")}
          </div>
          <div className="text-sm text-red-500 dark:text-red-400 mt-1">
            {error}
          </div>
        </div>
      )}

      {steps.length === 0 && !error && (
        <div className="flex items-center justify-center py-12">
          <div className="flex gap-2 items-center">
            <LoadingOutlined spin />
            <span className="text-slate-500">
              {t("proxy.debug.fetchingSource")}
            </span>
          </div>
        </div>
      )}

      {steps.length > 0 && (
        <div className="space-y-0">
          {steps.map((step, index) => {
            const isLast = index === steps.length - 1;
            const status = getStepStatus(step);
            const color =
              step.type === "validate" && step.data.skipped
                ? "#a1a1aa"
                : status === "error"
                  ? "#ef4444"
                  : status === "process"
                    ? "#3b82f6"
                    : "#22c55e";
            const stepKey =
              step.type === "source-start" || step.type === "source-result"
                ? `${step.type}-${step.data.sourceIndex}`
                : step.type;

            return (
              <div key={stepKey} className="flex gap-3">
                <div className="flex flex-col items-center">
                  <div
                    className="flex items-center justify-center w-6 h-6 text-lg shrink-0"
                    style={{ color }}
                  >
                    {getStepIcon(step)}
                  </div>
                  {!isLast && (
                    <div className="w-0.5 flex-1 bg-gray-200 dark:bg-gray-700 my-1" />
                  )}
                </div>
                <div className="flex-1 pb-4">
                  <div className="font-semibold text-sm">
                    {getStepLabel(step)}
                  </div>
                  <div className="mt-2 mb-4">{renderStepContent(step)}</div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {!done && steps.length > 0 && !error && (
        <div className="flex items-center gap-2 py-2 pl-8">
          <LoadingOutlined spin />
          <span className="text-slate-500 text-xs">{t("common.loading")}</span>
        </div>
      )}

      {/* 工具栏 */}
      {done && subscribeId && (
        <>
          <hr className="my-4 border-gray-200 dark:border-gray-700" />
          <div className="flex items-center gap-2 flex-wrap">
            {/* 节点追踪按钮 */}
            {allNodeNames.length > 0 && (
              <button
                type="button"
                className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md border border-gray-200 dark:border-gray-700 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors cursor-pointer"
                onClick={() => traceModalRef.current?.open()}
              >
                <AimOutlined className="text-blue-500" />
                <span>{t("proxy.debug.traceTitle")}</span>
                <Tag className="!text-xs">{allNodeNames.length}</Tag>
              </button>
            )}

            {/* 全局搜索按钮 */}
            <button
              type="button"
              className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md border border-gray-200 dark:border-gray-700 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors cursor-pointer"
              onClick={() => globalSearchRef.current?.open()}
            >
              <SearchOutlined className="text-green-500" />
              <span>{t("proxy.debug.globalSearch")}</span>
            </button>
          </div>

          <NodeTraceModal
            ref={traceModalRef}
            subscribeId={subscribeId}
            format={format}
            allNodeNames={allNodeNames}
            nodeWarnings={nodeWarningSet}
            nodeIgnored={nodeIgnoredSet}
          />
          <GlobalSearchModal ref={globalSearchRef} steps={steps} />
        </>
      )}

      <div ref={bottomRef} />
    </Modal>
  );
});

ProxyDebugModal.displayName = "ProxyDebugModal";

export default ProxyDebugModal;
