import {
  AimOutlined,
  Button,
  Collapse,
  Descriptions,
  Table,
  Tag,
  Tooltip,
} from "@acme/components";
import type {
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
