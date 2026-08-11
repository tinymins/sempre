import {
  BugOutlined,
  Button,
  CheckCircleOutlined,
  CloseCircleOutlined,
  Descriptions,
  LoadingOutlined,
  Modal,
  PlayCircleOutlined,
  SegmentedToggle,
  Tag,
} from "@acme/components";
import type {
  ProxySourceDebugMode,
  ProxySourceDebugStep,
  SubscribeItem,
} from "@acme/types";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { proxyApi } from "@/generated/rust-api";
import { useIsMobile } from "@/hooks";
import { useSourceDebugStepRenderer } from "./SourceDebugStepContent";

interface Props {
  open: boolean;
  item: SubscribeItem;
  onClose: () => void;
}


const SourceDebugModal = ({ open, item, onClose }: Props) => {
  const { t } = useTranslation();
  const isMobile = useIsMobile();
  const [mode, setMode] = useState<ProxySourceDebugMode>("bypass-cache");
  const [started, setStarted] = useState(false);
  const [running, setRunning] = useState(false);
  const [steps, setSteps] = useState<ProxySourceDebugStep[]>([]);
  const [streamError, setStreamError] = useState<string | null>(null);
  const controllerRef = useRef<AbortController | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  const reset = useCallback((keepMode = false) => {
    controllerRef.current?.abort();
    controllerRef.current = null;
    if (!keepMode) setMode("bypass-cache");
    setStarted(false);
    setRunning(false);
    setSteps([]);
    setStreamError(null);
  }, []);

  useEffect(() => () => controllerRef.current?.abort(), []);

  const startDebug = useCallback(() => {
    controllerRef.current?.abort();
    const controller = new AbortController();
    controllerRef.current = controller;
    setStarted(true);
    setRunning(true);
    setSteps([]);
    setStreamError(null);

    let completed = false;
    proxyApi.debugSource
      .stream(
        {
          url: item.url.trim(),
          ua: item.fetchUa || undefined,
          prefix: item.prefix || undefined,
          cacheTtlMinutes: item.cacheTtlMinutes,
          mode,
          fetchMode: item.fetchMode ?? "auto",
        },
        (step) => {
          setSteps((previous) => {
            if (step.type === "attempt-result") {
              return [
                ...previous.filter(
                  (existing) =>
                    !(
                      existing.type === "attempt-start" &&
                      existing.data.attempt === step.data.attempt
                    ),
                ),
                step,
              ];
            }
            return [...previous, step];
          });
          if (step.type === "done") {
            completed = true;
            setRunning(false);
          }
          setTimeout(
            () => bottomRef.current?.scrollIntoView({ behavior: "smooth" }),
            80,
          );
        },
        controller.signal,
      )
      .then(() => {
        if (!completed && !controller.signal.aborted) {
          setRunning(false);
          setStreamError(t("proxy.sourceDebug.streamEnded"));
        }
      })
      .catch((error: unknown) => {
        if (
          error instanceof Error &&
          error.name !== "AbortError" &&
          !controller.signal.aborted
        ) {
          setRunning(false);
          setStreamError(error.message);
        }
      });
  }, [item, mode, t]);

  const handleClose = () => {
    controllerRef.current?.abort();
    onClose();
  };

  const handleAfterOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) reset();
  };

  const restart = () => reset(true);

  const stepLabel = (step: ProxySourceDebugStep) => {
    switch (step.type) {
      case "config":
        return t("proxy.sourceDebug.requestConfig");
      case "cache":
        return t("proxy.sourceDebug.cacheCheck");
      case "network":
        return t("proxy.sourceDebug.networkDiagnostics");
      case "attempt-start":
      case "attempt-result":
        return t("proxy.sourceDebug.requestAttempt", {
          current: step.data.attempt,
          total: step.data.maxAttempts,
        });
      case "fallback":
        return t("proxy.sourceDebug.staleFallback");
      case "done":
        return t("proxy.sourceDebug.complete");
    }
  };

  const stepStatus = (
    step: ProxySourceDebugStep,
  ): "process" | "finish" | "error" | "warning" => {
    if (step.type === "attempt-start") return "process";
    if (step.type === "attempt-result" && !step.data.success) return "error";
    if (
      step.type === "network" &&
      (step.data.dnsError ||
        step.data.tcpProbes.some((probe) => !probe.success))
    )
      return "error";
    if (
      step.type === "cache" &&
      (step.data.status === "expired" || step.data.status === "unusable")
    )
      return "warning";
    if (
      step.type === "fallback" &&
      (step.data.status === "miss" || step.data.status === "unusable")
    )
      return "error";
    if (step.type === "done" && !step.data.success) return "error";
    return "finish";
  };

  const renderStep = useSourceDebugStepRenderer();

  return (
    <Modal
      title={
        <div className="flex items-center gap-2">
          <BugOutlined />
          <span>{t("proxy.sourceDebug.title")}</span>
          <Tag color="processing">
            {t(`proxy.sourceDebug.modeValue.${mode}`)}
          </Tag>
        </div>
      }
      open={open}
      onCancel={handleClose}
      afterOpenChange={handleAfterOpenChange}
      size="full"
      destroyOnClose
      footer={
        <div className="flex justify-end gap-2">
          {started && (
            <Button onClick={restart} disabled={running}>
              {t("proxy.sourceDebug.restart")}
            </Button>
          )}
          <Button onClick={handleClose}>{t("proxy.form.close")}</Button>
          {!started && (
            <Button
              variant="primary"
              icon={<PlayCircleOutlined />}
              onClick={startDebug}
              disabled={!item.url.trim()}
            >
              {t("proxy.sourceDebug.start")}
            </Button>
          )}
        </div>
      }
    >
      {!started ? (
        <div className="mx-auto flex w-full max-w-3xl flex-col gap-5 py-4">
          <Descriptions
            size="small"
            column={isMobile ? 1 : 2}
            bordered
            items={[
              {
                label: t("proxy.sourceDebug.url"),
                children: <span className="break-all text-xs">{item.url}</span>,
                span: isMobile ? 1 : 2,
              },
              {
                label: t("proxy.sourceDebug.userAgent"),
                children: item.fetchUa || "clash.meta",
              },
              {
                label: t("proxy.sourceDebug.cacheTtl"),
                children: `${item.cacheTtlMinutes ?? 60} min`,
              },
              {
                label: t("proxy.sourceDebug.prefix"),
                children: item.prefix || "-",
              },
              {
                label: t("proxy.sourceDebug.fetchMode"),
                children: t(
                  `proxy.sourceDebug.fetchModeValue.${item.fetchMode ?? "auto"}`,
                ),
              },
            ]}
          />
          <div>
            <div className="mb-2 text-sm font-medium">
              {t("proxy.sourceDebug.mode")}
            </div>
            <SegmentedToggle
              value={mode === "bypass-cache"}
              onChange={(bypass) =>
                setMode(bypass ? "bypass-cache" : "production")
              }
              checkedLabel={t("proxy.sourceDebug.modeValue.bypass-cache")}
              uncheckedLabel={t("proxy.sourceDebug.modeValue.production")}
            />
          </div>
        </div>
      ) : (
        <>
          {streamError && (
            <div className="mb-4 rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-800 dark:bg-red-950/30 dark:text-red-300">
              {streamError}
            </div>
          )}
          <div>
            {steps.map((step, index) => {
              const status = stepStatus(step);
              const color =
                status === "error"
                  ? "#ef4444"
                  : status === "warning"
                    ? "#f59e0b"
                    : status === "process"
                      ? "#3b82f6"
                      : "#22c55e";
              const key =
                step.type === "attempt-start" || step.type === "attempt-result"
                  ? `${step.type}-${step.data.attempt}`
                  : step.type;
              return (
                <div key={key} className="flex gap-3">
                  <div className="flex flex-col items-center">
                    <div
                      className="flex h-6 w-6 shrink-0 items-center justify-center text-lg"
                      style={{ color }}
                    >
                      {status === "process" ? (
                        <LoadingOutlined spin />
                      ) : status === "error" ? (
                        <CloseCircleOutlined />
                      ) : (
                        <CheckCircleOutlined />
                      )}
                    </div>
                    {index < steps.length - 1 && (
                      <div className="my-1 w-0.5 flex-1 bg-gray-200 dark:bg-gray-700" />
                    )}
                  </div>
                  <div className="min-w-0 flex-1 pb-5">
                    <div className="text-sm font-semibold">
                      {stepLabel(step)}
                    </div>
                    <div className="mt-2">{renderStep(step)}</div>
                  </div>
                </div>
              );
            })}
            {running && steps.length === 0 && (
              <div className="flex items-center justify-center gap-2 py-12 text-gray-500">
                <LoadingOutlined spin />
                {t("proxy.sourceDebug.requesting")}
              </div>
            )}
          </div>
          <div ref={bottomRef} />
        </>
      )}
    </Modal>
  );
};

export default SourceDebugModal;
