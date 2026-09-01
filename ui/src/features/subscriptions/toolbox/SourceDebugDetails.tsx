import {
  Collapse,
  Descriptions,
  Table,
  Tag,
} from "@acme/components";
import type {
  ProxyPreviewNode,
  ProxySourceDebugPayload,
} from "@acme/types";
import { useTranslation } from "react-i18next";
import { useIsMobile } from "@/hooks";
import { compareText } from "@/lib/sort";
import { SmartCodeBlock } from "./ProxyDebugModal/DebugStepContent";

export const PayloadDetails = ({
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

export const NodeTable = ({ nodes }: { nodes: ProxyPreviewNode[] }) => {
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
          sorter: (left, right) => compareText(left.name, right.name),
          ellipsis: true,
        },
        {
          title: t("proxy.preview.protocol"),
          dataIndex: "type",
          width: 90,
          sorter: (left, right) => compareText(left.type, right.type),
          render: (value: string) => <Tag>{value}</Tag>,
        },
        {
          title: t("proxy.preview.server"),
          dataIndex: "server",
          sorter: (left, right) => compareText(left.server, right.server),
          ellipsis: true,
        },
        {
          title: t("proxy.preview.port"),
          dataIndex: "port",
          width: 80,
          sorter: (left, right) => left.port - right.port,
        },
      ]}
    />
  );
};
