import {
  BugOutlined,
  Button,
  CheckCircleOutlined,
  CloseCircleOutlined,
  Collapse,
  Descriptions,
  LoadingOutlined,
  Modal,
  PlayCircleOutlined,
  SegmentedToggle,
  Table,
  Tag,
} from "@acme/components";
import type {
  ProxyPreviewNode,
  ProxySourceDebugMode,
  ProxySourceDebugPayload,
  ProxySourceDebugStep,
  SubscribeItem,
} from "@acme/types";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { proxyApi } from "@/generated/rust-api";
import { useIsMobile } from "@/hooks";
import { SmartCodeBlock } from "./ProxyDebugModal/DebugStepContent";

interface Props {
  open: boolean;
  item: SubscribeItem;
  onClose: () => void;
}

const PayloadDetails = ({
  payload,
  headers,
}: {
  payload: ProxySourceDebugPayload;
  headers?: Record<string, string>;
}) => {
  const { t } = useTranslation();
  const isMobile = useIsMobile();

  return (
    <div className="flex flex-col gap-2">
      <Descriptions
        size="small"
        column={isMobile ? 1 : 4}
        bordered
        items={[
          {
            label: t("proxy.sourceDebug.detectedFormat"),
            children: <Tag color="processing">{payload.format}</Tag>,
          },
          {
            label: t("proxy.sourceDebug.bodySize"),
            children: <>{payload.bodyBytes} B</>,
          },
          {
            label: t("proxy.sourceDebug.parsedNodes"),
            children: <Tag color="green">{payload.parsedNodeCount}</Tag>,
          },
          {
            label: t("proxy.sourceDebug.discardedNodes"),
            children: (
              <Tag
                color={
                  payload.discardedPlaceholderNodes.length > 0
                    ? "orange"
                    : "default"
                }
              >
                {payload.discardedPlaceholderNodes.length}
              </Tag>
            ),
          },
        ]}
      />

      {payload.diagnostics.length > 0 && (
        <div className="rounded-md border border-amber-200 bg-amber-50 p-3 text-xs text-amber-800 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-300">
          <div className="mb-1 font-semibold">
            {t("proxy.sourceDebug.parseDiagnostics")}
          </div>
          <ul className="m-0 list-disc space-y-1 pl-5">
            {payload.diagnostics.map((diagnostic) => (
              <li key={diagnostic}>{diagnostic}</li>
            ))}
          </ul>
        </div>
      )}

      <Collapse
        size="small"
        items={[
          ...(headers && Object.keys(headers).length > 0
            ? [
                {
                  key: "headers",
                  label: t("proxy.sourceDebug.responseHeaders"),
                  children: (
                    <SmartCodeBlock
                      content={JSON.stringify(headers, null, 2)}
                      maxHeight={300}
                    />
                  ),
                },
              ]
            : []),
          {
            key: "raw",
            label: (
              <div className="flex items-center gap-2">
                <span>{t("proxy.sourceDebug.rawResponse")}</span>
                <Tag>{payload.rawText.length}</Tag>
              </div>
            ),
            children: (
              <SmartCodeBlock content={payload.rawText} maxHeight={360} />
            ),
          },
          ...(payload.decodedText
            ? [
                {
                  key: "decoded",
                  label: (
                    <div className="flex items-center gap-2">
                      <span>{t("proxy.sourceDebug.decodedResponse")}</span>
                      <Tag>{payload.decodedText.length}</Tag>
                    </div>
                  ),
                  children: (
                    <SmartCodeBlock
                      content={payload.decodedText}
                      maxHeight={360}
                    />
                  ),
                },
              ]
            : []),
          ...(payload.discardedPlaceholderNodes.length > 0
            ? [
                {
                  key: "discarded",
                  label: (
                    <div className="flex items-center gap-2">
                      <span>{t("proxy.sourceDebug.discardedNodes")}</span>
                      <Tag color="orange">
                        {payload.discardedPlaceholderNodes.length}
                      </Tag>
                    </div>
                  ),
                  children: (
                    <NodeTable nodes={payload.discardedPlaceholderNodes} />
                  ),
                },
              ]
            : []),
          {
            key: "nodes",
            label: (
              <div className="flex items-center gap-2">
                <span>{t("proxy.sourceDebug.nodes")}</span>
                <Tag color="green">{payload.nodes.length}</Tag>
              </div>
            ),
            children: <NodeTable nodes={payload.nodes} />,
          },
        ]}
      />
    </div>
  );
};

const NodeTable = ({ nodes }: { nodes: ProxyPreviewNode[] }) => {
  const { t } = useTranslation();
  return (
    <Table
      size="small"
      pagination={false}
      scroll={{ x: 720, y: 320 }}
      dataSource={nodes}
      rowKey={(record) =>
        `${record.type}:${record.server}:${record.port}:${record.name}`
      }
      columns={[
        {
          title: t("proxy.preview.nodeName"),
          dataIndex: "name",
          ellipsis: true,
        },
        {
          title: t("proxy.preview.protocol"),
          dataIndex: "type",
          width: 90,
          render: (value: string) => <Tag>{value}</Tag>,
        },
        {
          title: t("proxy.preview.server"),
          dataIndex: "server",
          ellipsis: true,
        },
        {
          title: t("proxy.preview.port"),
          dataIndex: "port",
          width: 80,
        },
      ]}
    />
  );
};

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

  const cacheStatusLabel = (
    status: Extract<ProxySourceDebugStep, { type: "cache" }>["data"]["status"],
  ) => t(`proxy.sourceDebug.cacheStatus.${status}`);

  const fallbackStatusLabel = (
    status: Extract<
      ProxySourceDebugStep,
      { type: "fallback" }
    >["data"]["status"],
  ) => t(`proxy.sourceDebug.fallbackStatus.${status}`);

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

  const renderStep = (step: ProxySourceDebugStep) => {
    switch (step.type) {
      case "config":
        return (
          <Descriptions
            size="small"
            column={isMobile ? 1 : 3}
            bordered
            items={[
              {
                label: t("proxy.sourceDebug.url"),
                children: (
                  <span className="break-all text-xs">{step.data.url}</span>
                ),
                span: isMobile ? 1 : 2,
              },
              {
                label: t("proxy.sourceDebug.mode"),
                children: (
                  <Tag color="blue">
                    {t(`proxy.sourceDebug.modeValue.${step.data.mode}`)}
                  </Tag>
                ),
              },
              {
                label: t("proxy.sourceDebug.fetchMode"),
                children: (
                  <Tag color={step.data.fetchMode === "auto" ? "blue" : "gold"}>
                    {t(
                      `proxy.sourceDebug.fetchModeValue.${step.data.fetchMode}`,
                    )}
                  </Tag>
                ),
              },
              {
                label: t("proxy.sourceDebug.proxyEndpoint"),
                children: (
                  <span className="break-all text-xs">
                    {step.data.proxyEndpoint ?? "-"}
                  </span>
                ),
              },
              {
                label: t("proxy.sourceDebug.userAgent"),
                children: (
                  <span className="break-all text-xs">{step.data.ua}</span>
                ),
                span: 2,
              },
              {
                label: t("proxy.sourceDebug.prefix"),
                children: step.data.prefix || "-",
              },
              {
                label: t("proxy.sourceDebug.cacheTtl"),
                children: `${step.data.cacheTtlMinutes} min`,
              },
              {
                label: t("proxy.sourceDebug.maxAttempts"),
                children: step.data.maxAttempts,
              },
              {
                label: t("proxy.sourceDebug.timeout"),
                children: `${step.data.timeoutMs} ms`,
              },
            ]}
          />
        );
      case "cache":
        return (
          <div className="flex flex-col gap-2">
            <Descriptions
              size="small"
              column={isMobile ? 1 : 2}
              bordered
              items={[
                {
                  label: t("proxy.sourceDebug.cacheStatusLabel"),
                  children: (
                    <Tag
                      color={
                        step.data.status === "hit"
                          ? "green"
                          : step.data.status === "unusable"
                            ? "orange"
                            : "default"
                      }
                    >
                      {cacheStatusLabel(step.data.status)}
                    </Tag>
                  ),
                },
                {
                  label: t("proxy.sourceDebug.cacheTtl"),
                  children: `${step.data.cacheTtlMinutes} min`,
                },
              ]}
            />
            {step.data.payload && (
              <PayloadDetails payload={step.data.payload} />
            )}
          </div>
        );
      case "attempt-start":
        return (
          <div className="flex items-center gap-2 text-sm text-gray-500">
            <LoadingOutlined spin />
            {t("proxy.sourceDebug.requesting")}
          </div>
        );
      case "attempt-result":
        return (
          <div className="flex flex-col gap-2">
            <Descriptions
              size="small"
              column={isMobile ? 1 : 4}
              bordered
              items={[
                {
                  label: t("proxy.sourceDebug.result"),
                  children: step.data.success ? (
                    <Tag color="success">{t("proxy.sourceDebug.success")}</Tag>
                  ) : (
                    <Tag color="error">{t("proxy.sourceDebug.failed")}</Tag>
                  ),
                },
                {
                  label: t("proxy.sourceDebug.httpStatus"),
                  children: step.data.httpStatus ?? "-",
                },
                {
                  label: t("proxy.sourceDebug.duration"),
                  children: `${step.data.fetchDurationMs} ms`,
                },
                {
                  label: t("proxy.sourceDebug.finalUrl"),
                  children: (
                    <span className="break-all text-xs">
                      {step.data.finalUrl ?? "-"}
                    </span>
                  ),
                  span: isMobile ? 1 : 4,
                },
                {
                  label: t("proxy.sourceDebug.remoteAddress"),
                  children: step.data.remoteAddress ?? "-",
                },
                {
                  label: t("proxy.sourceDebug.httpVersion"),
                  children: step.data.httpVersion ?? "-",
                },
                {
                  label: t("proxy.sourceDebug.tlsCertificate"),
                  children:
                    step.data.tlsPeerCertificateBytes === null
                      ? "-"
                      : `${step.data.tlsPeerCertificateBytes} B`,
                  span: isMobile ? 1 : 2,
                },
              ]}
            />
            {step.data.error && (
              <div className="rounded-md border border-red-200 bg-red-50 p-3 text-xs text-red-700 dark:border-red-800 dark:bg-red-950/30 dark:text-red-300">
                {step.data.error}
              </div>
            )}
            {step.data.requestError && (
              <div className="rounded-md border border-red-200 bg-red-50 p-3 text-xs text-red-800 dark:border-red-800 dark:bg-red-950/30 dark:text-red-200">
                <div className="mb-2 font-semibold">
                  {t("proxy.sourceDebug.requestErrorDetails")}
                </div>
                <div className="mb-2 flex flex-wrap gap-1">
                  {(
                    [
                      ["timeout", step.data.requestError.isTimeout],
                      ["connect", step.data.requestError.isConnect],
                      ["request", step.data.requestError.isRequest],
                      ["body", step.data.requestError.isBody],
                      ["decode", step.data.requestError.isDecode],
                    ] as const
                  )
                    .filter(([, enabled]) => enabled)
                    .map(([category]) => (
                      <Tag key={category} color="error">
                        {category}
                      </Tag>
                    ))}
                </div>
                <ol className="m-0 list-decimal space-y-1 pl-5">
                  {[...new Set(step.data.requestError.chain)].map((cause) => (
                    <li key={cause} className="break-all">
                      {cause}
                    </li>
                  ))}
                </ol>
                <Collapse
                  className="mt-2"
                  size="small"
                  items={[
                    {
                      key: "debug",
                      label: t("proxy.sourceDebug.rawError"),
                      children: (
                        <SmartCodeBlock
                          content={step.data.requestError.debug}
                          maxHeight={320}
                        />
                      ),
                    },
                  ]}
                />
              </div>
            )}
            <PayloadDetails
              payload={step.data.payload}
              headers={step.data.httpHeaders}
            />
          </div>
        );
      case "network":
        return (
          <div className="flex flex-col gap-2">
            <Descriptions
              size="small"
              column={isMobile ? 1 : 4}
              bordered
              items={[
                {
                  label: t(
                    step.data.connectionKind === "proxy"
                      ? "proxy.sourceDebug.proxyHost"
                      : "proxy.sourceDebug.targetHost",
                  ),
                  children: (
                    <span className="break-all text-xs">
                      {step.data.host
                        ? `${step.data.host}:${step.data.port ?? "-"}`
                        : "-"}
                    </span>
                  ),
                  span: isMobile ? 1 : 2,
                },
                {
                  label: t("proxy.sourceDebug.connectionKind"),
                  children: t(
                    `proxy.sourceDebug.connectionKindValue.${step.data.connectionKind}`,
                  ),
                },
                {
                  label: t("proxy.sourceDebug.proxyEndpoint"),
                  children: (
                    <span className="break-all text-xs">
                      {step.data.proxyEndpoint ?? "-"}
                    </span>
                  ),
                  span: isMobile ? 1 : 2,
                },
                {
                  label: t("proxy.sourceDebug.scheme"),
                  children: step.data.scheme ?? "-",
                },
                {
                  label: t("proxy.sourceDebug.dnsDuration"),
                  children: `${step.data.dnsDurationMs} ms`,
                },
                {
                  label: t("proxy.sourceDebug.resolvedAddresses"),
                  children:
                    step.data.resolvedAddresses.length > 0 ? (
                      <div className="flex flex-wrap gap-1">
                        {step.data.resolvedAddresses.map((address) => (
                          <Tag key={address} color="blue">
                            {address}
                          </Tag>
                        ))}
                      </div>
                    ) : (
                      "-"
                    ),
                  span: isMobile ? 1 : 4,
                },
                {
                  label: t("proxy.sourceDebug.proxyEnvironment"),
                  children:
                    step.data.proxyEnvironmentVariables.length > 0
                      ? step.data.proxyEnvironmentVariables.join(", ")
                      : t("proxy.sourceDebug.none"),
                  span: isMobile ? 1 : 4,
                },
              ]}
            />
            {step.data.dnsError && (
              <div className="rounded-md border border-red-200 bg-red-50 p-3 text-xs text-red-700 dark:border-red-800 dark:bg-red-950/30 dark:text-red-300">
                {t("proxy.sourceDebug.dnsError")}: {step.data.dnsError}
              </div>
            )}
            <Collapse
              size="small"
              defaultActiveKey={["tcp"]}
              items={[
                {
                  key: "resolver",
                  label: t("proxy.sourceDebug.resolverConfig"),
                  children: (
                    <SmartCodeBlock
                      content={
                        step.data.resolverConfig.join("\n") ||
                        t("proxy.sourceDebug.none")
                      }
                      maxHeight={220}
                    />
                  ),
                },
                {
                  key: "tcp",
                  label: `${t("proxy.sourceDebug.tcpProbes")} (${step.data.tcpProbes.length})`,
                  children: (
                    <Table
                      size="small"
                      pagination={false}
                      scroll={{ x: 760 }}
                      dataSource={step.data.tcpProbes}
                      rowKey="address"
                      columns={[
                        {
                          title: t("proxy.sourceDebug.address"),
                          dataIndex: "address",
                          width: 190,
                        },
                        {
                          title: t("proxy.sourceDebug.result"),
                          dataIndex: "success",
                          width: 80,
                          render: (success: boolean) => (
                            <Tag color={success ? "success" : "error"}>
                              {success
                                ? t("proxy.sourceDebug.success")
                                : t("proxy.sourceDebug.failed")}
                            </Tag>
                          ),
                        },
                        {
                          title: t("proxy.sourceDebug.duration"),
                          dataIndex: "durationMs",
                          width: 90,
                          render: (duration: number) => `${duration} ms`,
                        },
                        {
                          title: t("proxy.sourceDebug.localAddress"),
                          dataIndex: "localAddress",
                          width: 190,
                          render: (value: string | null) => value ?? "-",
                        },
                        {
                          title: t("proxy.sourceDebug.error"),
                          dataIndex: "error",
                          render: (value: string | null) => value ?? "-",
                        },
                      ]}
                    />
                  ),
                },
              ]}
            />
          </div>
        );
      case "fallback":
        return (
          <div className="flex flex-col gap-2">
            <Tag
              color={step.data.status === "hit" ? "green" : "error"}
              className="w-fit"
            >
              {fallbackStatusLabel(step.data.status)}
            </Tag>
            {step.data.payload && (
              <PayloadDetails payload={step.data.payload} />
            )}
          </div>
        );
      case "done":
        return (
          <Descriptions
            size="small"
            column={isMobile ? 1 : 4}
            bordered
            items={[
              {
                label: t("proxy.sourceDebug.result"),
                children: step.data.success ? (
                  <Tag color="success">{t("proxy.sourceDebug.success")}</Tag>
                ) : (
                  <Tag color="error">{t("proxy.sourceDebug.failed")}</Tag>
                ),
              },
              {
                label: t("proxy.sourceDebug.resultSource"),
                children: step.data.resultSource
                  ? t(
                      `proxy.sourceDebug.resultSourceValue.${step.data.resultSource}`,
                    )
                  : "-",
              },
              {
                label: t("proxy.sourceDebug.parsedNodes"),
                children: step.data.nodeCount,
              },
              {
                label: t("proxy.sourceDebug.duration"),
                children: `${step.data.totalDurationMs} ms`,
              },
            ]}
          />
        );
    }
  };

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
