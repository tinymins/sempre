import {
  AimOutlined,
  Button,
  CheckCircleOutlined,
  CloseCircleOutlined,
  Collapse,
  Descriptions,
  LoadingOutlined,
  ShieldCheckOutlined,
  Table,
  Tag,
  Tooltip,
} from "@acme/components";
import type {
  ProxyDebugFilteredNode,
  ProxyDebugStep,
  ProxyPreviewNode,
} from "@acme/types";
import { useTranslation } from "react-i18next";
import { SyntaxJsonViewer } from "./InteractiveJsonViewer";

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

/** Try JSON parse → SyntaxJsonViewer, fallback to plain CodeBlock */
export const SmartCodeBlock = ({
  content,
  maxHeight,
}: {
  content: string;
  maxHeight?: number;
}) => {
  let parsed: unknown;
  try {
    parsed = JSON.parse(content);
  } catch {
    return <CodeBlock content={content} maxHeight={maxHeight} />;
  }
  return <SyntaxJsonViewer data={parsed} maxHeight={maxHeight} />;
};

/** 配置解析步骤 */
export const ConfigStepContent = ({
  step,
}: {
  step: Extract<ProxyDebugStep, { type: "config" }>;
}) => {
  const { t } = useTranslation();
  const { data } = step;

  return (
    <Collapse
      size="small"
      items={[
        {
          key: "urls",
          label: (
            <div className="flex gap-2 items-center">
              <span>{t("proxy.debug.subscribeUrls")}</span>
              <Tag color="blue">{data.subscribeUrls.length}</Tag>
            </div>
          ),
          children: (
            <div className="flex flex-col gap-1">
              {data.subscribeUrls.map((url) => (
                <code
                  key={url}
                  className="text-xs bg-gray-100 dark:bg-gray-800 px-1 py-0.5 rounded break-all"
                >
                  {url}
                </code>
              ))}
            </div>
          ),
        },
        {
          key: "filters",
          label: (
            <div className="flex gap-2 items-center">
              <span>{t("proxy.debug.filterRules")}</span>
              <Tag color="orange">{data.filters.length}</Tag>
            </div>
          ),
          children:
            data.filters.length > 0 ? (
              <div className="flex flex-wrap gap-1">
                {data.filters.map((f: string) => (
                  <Tag key={f}>{f}</Tag>
                ))}
              </div>
            ) : (
              <span className="text-slate-500">-</span>
            ),
        },
        {
          key: "groups",
          label: (
            <div className="flex gap-2 items-center">
              <span>{t("proxy.debug.groupConfig")}</span>
              <Tag color="purple">{data.groups.length}</Tag>
            </div>
          ),
          children: <SyntaxJsonViewer data={data.groups} />,
        },
        {
          key: "ruleProviders",
          label: (
            <div className="flex gap-2 items-center">
              <span>{t("proxy.debug.ruleProviders")}</span>
              <Tag color="cyan">{Object.keys(data.ruleProviders).length}</Tag>
            </div>
          ),
          children: <SyntaxJsonViewer data={data.ruleProviders} />,
        },
        {
          key: "customConfig",
          label: (
            <div className="flex gap-2 items-center">
              <span>{t("proxy.debug.customConfigRules")}</span>
              <Tag>{data.customConfig.length}</Tag>
            </div>
          ),
          children: <SyntaxJsonViewer data={data.customConfig} />,
        },
        {
          key: "servers",
          label: (
            <div className="flex gap-2 items-center">
              <span>{t("proxy.debug.manualServers")}</span>
              <Tag>{data.servers.length}</Tag>
            </div>
          ),
          children: <SyntaxJsonViewer data={data.servers} />,
        },
        {
          key: "privateAccessConfig",
          label: (
            <div className="flex gap-2 items-center">
              <span>{t("proxy.debug.privateAccessConfig")}</span>
              <Tag
                color={data.privateAccessConfig?.enabled ? "green" : "default"}
              >
                {data.privateAccessConfig?.enabled ? "enabled" : "disabled"}
              </Tag>
            </div>
          ),
          children: <SyntaxJsonViewer data={data.privateAccessConfig} />,
        },
        {
          key: "dnsConfig",
          label: (
            <div className="flex gap-2 items-center">
              <span>{t("proxy.debug.dnsConfig")}</span>
              <Tag color="geekblue">
                {Object.keys(data.dnsConfig.overrides).length > 0
                  ? `shared + ${Object.keys(data.dnsConfig.overrides).join(", ")}`
                  : "shared"}
              </Tag>
            </div>
          ),
          children: <SyntaxJsonViewer data={data.dnsConfig} />,
        },
      ]}
    />
  );
};

/** 本地服务器步骤 */
export const ManualServersStepContent = ({
  step,
  onTraceNode,
}: {
  step: Extract<ProxyDebugStep, { type: "manual-servers" }>;
  onTraceNode?: (nodeName: string) => void;
}) => {
  const { t } = useTranslation();
  const { data } = step;

  if (data.count === 0) {
    return (
      <span className="text-slate-500">{t("proxy.debug.noLocalServers")}</span>
    );
  }

  return (
    <div>
      <Descriptions
        size="small"
        column={1}
        bordered
        items={[
          {
            label: t("proxy.debug.serverCount"),
            children: <Tag color="blue">{data.count}</Tag>,
          },
        ]}
      />
      <div className="mt-2">
        <Table
          size="small"
          pagination={false}
          dataSource={data.nodes}
          rowKey={(record: ProxyPreviewNode) =>
            String(data.nodes.indexOf(record))
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
              width: 80,
              render: (v: string) => <Tag>{v}</Tag>,
            },
            {
              title: t("proxy.preview.server"),
              dataIndex: "server",
              ellipsis: true,
            },
            {
              title: t("proxy.preview.port"),
              dataIndex: "port",
              width: 70,
            },
            {
              title: t("proxy.debug.dataSource"),
              dataIndex: "sourceUrl",
              width: 120,
              render: (sourceUrl: string) => (
                <Tag color={sourceUrl.startsWith("custom-node:") ? "cyan" : "default"}>
                  {t(
                    sourceUrl.startsWith("custom-node:")
                      ? "proxy.debug.customServerSource"
                      : "proxy.debug.manualConfigSource",
                  )}
                </Tag>
              ),
            },
            ...(onTraceNode
              ? [
                  {
                    title: "",
                    width: 40,
                    render: (_: unknown, record: { name: string }) => (
                      <Tooltip title={t("proxy.debug.traceNode")}>
                        <Button
                          variant="text"
                          size="small"
                          icon={<AimOutlined />}
                          onClick={() => onTraceNode(record.name)}
                        />
                      </Tooltip>
                    ),
                  },
                ]
              : []),
          ]}
        />
      </div>
    </div>
  );
};

/** 正在获取订阅源 */
export const SourceStartStepContent = ({
  step,
}: {
  step: Extract<ProxyDebugStep, { type: "source-start" }>;
}) => {
  const { t } = useTranslation();

  return (
    <div className="flex items-center gap-2">
      <LoadingOutlined spin />
      <span>{t("proxy.debug.fetchingSource")}</span>
      <code className="text-xs bg-gray-100 dark:bg-gray-800 px-1 py-0.5 rounded break-all">
        {step.data.url}
      </code>
    </div>
  );
};

/** 订阅源获取结果 */
export const SourceResultStepContent = ({
  step,
  onTraceNode,
}: {
  step: Extract<ProxyDebugStep, { type: "source-result" }>;
  onTraceNode?: (nodeName: string) => void;
}) => {
  const { t } = useTranslation();
  const { data } = step;

  return (
    <div className="flex flex-col gap-2">
      {/* Basic info */}
      <Descriptions
        size="small"
        column={3}
        bordered
        items={[
          {
            label: t("proxy.debug.sourceUrl"),
            children: <span className="text-xs break-all">{data.url}</span>,
            span: 2,
          },
          {
            label: t("proxy.debug.httpStatus"),
            children: data.error ? (
              <Tag icon={<CloseCircleOutlined />} color="error">
                {t("proxy.debug.error")}
              </Tag>
            ) : (
              <Tag
                icon={<CheckCircleOutlined />}
                color={data.httpStatus === 200 ? "success" : "warning"}
              >
                {data.httpStatus}
              </Tag>
            ),
          },
          {
            label: t("proxy.debug.detectedFormat"),
            children: <Tag color="processing">{data.format}</Tag>,
          },
          {
            label: t("proxy.debug.dataSource"),
            children: data.cached ? (
              <Tag color="green">{t("proxy.debug.cached")}</Tag>
            ) : (
              <Tag color="blue">{t("proxy.debug.liveFetch")}</Tag>
            ),
          },
          {
            label: t("proxy.debug.fetchDuration"),
            children: <>{data.fetchDurationMs}ms</>,
          },
          {
            label: t("proxy.debug.parsedNodes"),
            children: <Tag color="blue">{data.parsedNodeCount}</Tag>,
          },
          {
            label: t("proxy.debug.afterFilter"),
            children: (
              <>
                <Tag color="green">{data.nodesAfterFilter.length}</Tag>
                {data.filteredNodes.length > 0 && (
                  <Tag color="orange" className="ml-1">
                    -{data.filteredNodes.length}
                  </Tag>
                )}
              </>
            ),
          },
        ]}
      />

      {data.error && <p className="text-xs text-red-500 !mb-0">{data.error}</p>}

      {/* Collapsible details */}
      <Collapse
        size="small"
        items={[
          {
            key: "raw",
            label: (
              <div className="flex gap-2 items-center">
                <span>{t("proxy.debug.rawResponse")}</span>
                <Tag>
                  {data.rawText.length} {t("proxy.debug.chars")}
                </Tag>
              </div>
            ),
            children: <SmartCodeBlock content={data.rawText} maxHeight={300} />,
          },
          ...(data.decodedText
            ? [
                {
                  key: "decoded",
                  label: (
                    <div className="flex gap-2 items-center">
                      <span>{t("proxy.debug.decodedResponse")}</span>
                      <Tag>
                        {data.decodedText.length} {t("proxy.debug.chars")}
                      </Tag>
                    </div>
                  ),
                  children: (
                    <SmartCodeBlock
                      content={data.decodedText}
                      maxHeight={300}
                    />
                  ),
                },
              ]
            : []),
          ...(data.filteredNodes.length > 0
            ? [
                {
                  key: "filtered",
                  label: (
                    <div className="flex gap-2 items-center">
                      <span>{t("proxy.debug.filteredNodes")}</span>
                      <Tag color="orange">{data.filteredNodes.length}</Tag>
                    </div>
                  ),
                  children: (
                    <Table
                      size="small"
                      pagination={false}
                      dataSource={data.filteredNodes}
                      rowKey={(record: ProxyDebugFilteredNode) =>
                        String(data.filteredNodes.indexOf(record))
                      }
                      columns={[
                        {
                          title: t("proxy.preview.nodeName"),
                          dataIndex: ["node", "name"],
                          ellipsis: true,
                        },
                        {
                          title: t("proxy.debug.matchedRule"),
                          dataIndex: "matchedRule",
                          width: 120,
                          render: (v: string) => <Tag color="orange">{v}</Tag>,
                        },
                        ...(onTraceNode
                          ? [
                              {
                                title: "",
                                width: 40,
                                render: (
                                  _: unknown,
                                  record: { node: { name: string } },
                                ) => (
                                  <Tooltip title={t("proxy.debug.traceNode")}>
                                    <Button
                                      variant="text"
                                      size="small"
                                      icon={<AimOutlined />}
                                      onClick={() =>
                                        onTraceNode(record.node.name)
                                      }
                                    />
                                  </Tooltip>
                                ),
                              },
                            ]
                          : []),
                      ]}
                    />
                  ),
                },
              ]
            : []),
          {
            key: "nodes",
            label: (
              <div className="flex gap-2 items-center">
                <span>{t("proxy.debug.nodesAfterFilter")}</span>
                <Tag color="green">{data.nodesAfterFilter.length}</Tag>
              </div>
            ),
            children: (
              <Table
                size="small"
                pagination={false}
                scroll={{ y: 300 }}
                dataSource={data.nodesAfterFilter}
                rowKey={(record: ProxyPreviewNode) =>
                  String(data.nodesAfterFilter.indexOf(record))
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
                    width: 80,
                    render: (v: string) => <Tag>{v}</Tag>,
                  },
                  {
                    title: t("proxy.preview.server"),
                    dataIndex: "server",
                    ellipsis: true,
                  },
                  {
                    title: t("proxy.preview.port"),
                    dataIndex: "port",
                    width: 70,
                  },
                  ...(onTraceNode
                    ? [
                        {
                          title: "",
                          width: 50,
                          render: (_: unknown, record: { name: string }) => (
                            <Tooltip title={t("proxy.debug.traceNode")}>
                              <Button
                                variant="link"
                                size="small"
                                icon={<AimOutlined />}
                                onClick={() => onTraceNode(record.name)}
                              />
                            </Tooltip>
                          ),
                        },
                      ]
                    : []),
                ]}
              />
            ),
          },
        ]}
      />
    </div>
  );
};

/** 节点合并步骤 */
export const MergeStepContent = ({
  step,
  onTraceNode,
}: {
  step: Extract<ProxyDebugStep, { type: "merge" }>;
  onTraceNode?: (nodeName: string) => void;
}) => {
  const { t } = useTranslation();
  const { data } = step;
  const warningSet = new Set(data.nodeWarnings ?? []);
  const ignoredSet = new Set(data.nodeIgnored ?? []);
  const warningCount = warningSet.size;
  const ignoredCount = ignoredSet.size;

  return (
    <div className="flex flex-col gap-2">
      <Descriptions
        size="small"
        column={warningCount > 0 || ignoredCount > 0 ? 4 : 3}
        bordered
        items={[
          {
            label: t("proxy.debug.totalNodes"),
            children: <Tag color="blue">{data.totalNodesBeforeFilter}</Tag>,
          },
          {
            label: t("proxy.debug.activeNodes"),
            children: <Tag color="green">{data.totalNodesAfterFilter}</Tag>,
          },
          {
            label: t("proxy.debug.filteredCount"),
            children: <Tag color="orange">{data.totalFiltered}</Tag>,
          },
          ...(warningCount > 0
            ? [
                {
                  label: t("proxy.debug.entropyWarning"),
                  children: <Tag color="gold">{warningCount}</Tag>,
                },
              ]
            : []),
        ]}
      />

      <Collapse
        size="small"
        items={[
          {
            key: "finalNodes",
            label: (
              <div className="flex gap-2 items-center">
                <span>{t("proxy.debug.nodeStats")}</span>
                <Tag>{data.finalNodeNames.length}</Tag>
                {warningCount > 0 && (
                  <Tag color="gold">
                    {warningCount} {t("proxy.debug.entropyWarningShort")}
                  </Tag>
                )}
              </div>
            ),
            children: (
              <div className="flex flex-wrap gap-1">
                {data.finalNodeNames.map((name: string) => {
                  const hasWarning = warningSet.has(name);
                  const hasIgnored = ignoredSet.has(name);
                  const tagColor = hasWarning
                    ? "gold"
                    : hasIgnored
                      ? "blue"
                      : "green";
                  const tooltipTitle = hasWarning
                    ? t("proxy.debug.entropyWarningTip")
                    : hasIgnored
                      ? t("proxy.debug.ignoredFieldsTip")
                      : t("proxy.debug.traceNode");
                  return onTraceNode ? (
                    <Tooltip key={name} title={tooltipTitle}>
                      <Tag
                        className="cursor-pointer"
                        color={tagColor}
                        onClick={() => onTraceNode(name)}
                      >
                        <AimOutlined className="mr-1" />
                        {name}
                      </Tag>
                    </Tooltip>
                  ) : (
                    <Tag key={name} color={tagColor}>
                      {name}
                    </Tag>
                  );
                })}
              </div>
            ),
          },
        ]}
      />
    </div>
  );
};

/** 配置构建步骤 */
export const OutputStepContent = ({
  step,
}: {
  step: Extract<ProxyDebugStep, { type: "output" }>;
}) => {
  const { t } = useTranslation();
  const { data } = step;

  return (
    <div className="flex flex-col gap-2">
      <Descriptions
        size="small"
        column={3}
        bordered
        items={[
          {
            label: t("proxy.debug.proxyGroups"),
            children: <Tag color="purple">{data.proxyGroupCount}</Tag>,
          },
          {
            label: t("proxy.debug.rules"),
            children: <Tag color="cyan">{data.ruleCount}</Tag>,
          },
          {
            label: t("proxy.debug.ruleProviders"),
            children: <Tag>{data.ruleProviderCount}</Tag>,
          },
        ]}
      />

      <Collapse
        size="small"
        items={[
          {
            key: "config",
            label: (
              <div className="flex gap-2 items-center">
                <span>{t("proxy.debug.finalConfig")}</span>
                <Tag>
                  {data.configOutput.length} {t("proxy.debug.chars")}
                </Tag>
              </div>
            ),
            children: (
              <SmartCodeBlock content={data.configOutput} maxHeight={500} />
            ),
          },
        ]}
      />
    </div>
  );
};

/** 方法名称映射 */
const getValidateMethodLabel = (
  method: string | undefined,
  t: (key: string) => string,
): string => {
  switch (method) {
    case "sing-box-binary":
      return t("proxy.debug.validateMethodSingbox");
    case "yaml-syntax":
      return t("proxy.debug.validateMethodYaml");
    default:
      return method ?? "";
  }
};

/** 配置校验步骤 */
export const ValidateStepContent = ({
  step,
}: {
  step: Extract<ProxyDebugStep, { type: "validate" }>;
}) => {
  const { t } = useTranslation();
  const { data } = step;

  if (data.skipped) {
    return (
      <div className="flex items-center gap-2 text-zinc-400">
        <ShieldCheckOutlined />
        <span>{t("proxy.debug.validateSkipped")}</span>
        {data.reason && (
          <span className="text-xs text-zinc-400">({data.reason})</span>
        )}
      </div>
    );
  }

  const warnings = data.warnings ?? [];
  const errors = data.errors ?? [];

  if (data.valid) {
    return (
      <div className="flex flex-col gap-2">
        <div className="flex items-center gap-2 text-green-500 text-sm">
          <CheckCircleOutlined />
          <span>{t("proxy.debug.validatePassed")}</span>
          {data.method && (
            <Tag color="green">{getValidateMethodLabel(data.method, t)}</Tag>
          )}
        </div>
        {warnings.length > 0 && (
          <div className="flex flex-col gap-1 ml-6">
            {warnings.map((w) => (
              <div
                key={w}
                className="text-xs text-yellow-500 break-all font-mono"
              >
                ⚠ {w}
              </div>
            ))}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2 text-red-500">
        <CloseCircleOutlined />
        <span className="font-medium">{t("proxy.debug.validateFailed")}</span>
        {data.method && (
          <Tag color="error">{getValidateMethodLabel(data.method, t)}</Tag>
        )}
      </div>
      {errors.length > 0 && (
        <div className="flex flex-col gap-1 ml-6 p-2 rounded-md bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800">
          {errors.map((e) => (
            <div
              key={e}
              className="text-xs text-red-500 dark:text-red-400 break-all font-mono"
            >
              ✕ {e}
            </div>
          ))}
        </div>
      )}
      {warnings.length > 0 && (
        <div className="flex flex-col gap-1 ml-6">
          {warnings.map((w) => (
            <div
              key={w}
              className="text-xs text-yellow-500 break-all font-mono"
            >
              ⚠ {w}
            </div>
          ))}
        </div>
      )}
    </div>
  );
};

/** 完成步骤 */
export const DoneStepContent = ({
  step,
}: {
  step: Extract<ProxyDebugStep, { type: "done" }>;
}) => {
  const { t } = useTranslation();

  return (
    <Descriptions
      size="small"
      column={1}
      items={[
        {
          label: t("proxy.debug.totalDuration"),
          children: (
            <Tag color="green">
              <CheckCircleOutlined className="mr-1" />
              {step.data.totalDurationMs}ms
            </Tag>
          ),
        },
      ]}
    />
  );
};
