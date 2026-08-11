import {
  Collapse,
  Descriptions,
  LoadingOutlined,
  Table,
  Tag,
} from "@acme/components";
import type { ProxySourceDebugStep } from "@acme/types";
import { useTranslation } from "react-i18next";
import { useIsMobile } from "@/hooks";
import { SmartCodeBlock } from "./ProxyDebugModal/DebugStepContent";
import { PayloadDetails } from "./SourceDebugDetails";

export function useSourceDebugStepRenderer() {
  const { t } = useTranslation();
  const isMobile = useIsMobile();
  const cacheStatusLabel = (
    status: Extract<ProxySourceDebugStep, { type: "cache" }>["data"]["status"],
  ) => t(`proxy.sourceDebug.cacheStatus.${status}`);

  const fallbackStatusLabel = (
    status: Extract<
      ProxySourceDebugStep,
      { type: "fallback" }
    >["data"]["status"],
  ) => t(`proxy.sourceDebug.fallbackStatus.${status}`);
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
  return renderStep;
}
