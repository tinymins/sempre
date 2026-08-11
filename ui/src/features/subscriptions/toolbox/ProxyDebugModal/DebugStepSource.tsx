import {
  AimOutlined,
  Button,
  CheckCircleOutlined,
  CloseCircleOutlined,
  Collapse,
  Descriptions,
  LoadingOutlined,
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
import { SmartCodeBlock } from "./DebugStepCommon";

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
