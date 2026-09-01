import type { DescriptionsItem, TableColumnsType } from "@acme/components";
import {
  Descriptions,
  Empty,
  EyeOutlined,
  Modal,
  Spin,
  Table,
  Tag,
  Tooltip,
} from "@acme/components";
import { forwardRef, useImperativeHandle, useState } from "react";
import { useTranslation } from "react-i18next";
import { proxyApi } from "@/generated/rust-api";
import { useIsMobile } from "@/hooks";
import { compareText } from "@/lib/sort";
import {
  formatValue,
  MobileNodeCard,
  type ProxyNode,
  type ProxyPreviewModalRef,
  protocolFields,
  typeColorMap,
} from "./ProxyPreviewContent";

const ProxyPreviewModal = forwardRef<ProxyPreviewModalRef>((_, ref) => {
  const { t } = useTranslation();
  const isMobile = useIsMobile();
  const [visible, setVisible] = useState(false);
  const [subscribeId, setSubscribeId] = useState<string>("");
  const [subscribeRemark, setSubscribeRemark] = useState<string>("");

  const { data, isLoading } = proxyApi.previewNodes.useQuery(
    { id: subscribeId, format: "clash" },
    { enabled: !!subscribeId && visible },
  );

  useImperativeHandle(ref, () => ({
    open: (id: string, remark?: string | null) => {
      setSubscribeId(id);
      setSubscribeRemark(remark ?? t("proxy.preview.unnamed"));
      setVisible(true);
    },
  }));

  const handleClose = () => {
    setVisible(false);
    setSubscribeId("");
  };

  const columns: TableColumnsType<ProxyNode> = [
    {
      title: t("proxy.preview.source"),
      dataIndex: "sourceIndex",
      sorter: (left, right) => left.sourceIndex - right.sourceIndex,
      width: 80,
      align: "center",
      render: (index: number, record) => (
        <Tooltip title={record.sourceUrl} placement="top">
          <Tag color={index === 0 ? "default" : "blue"} className="cursor-help">
            {index === 0 ? t("proxy.preview.manual") : `#${index}`}
          </Tag>
        </Tooltip>
      ),
    },
    {
      title: t("proxy.preview.protocol"),
      dataIndex: "type",
      sorter: (left, right) => compareText(left.type, right.type),
      width: 100,
      align: "center",
      render: (type: string, record) => (
        <Tag
          color={record.filtered ? "default" : typeColorMap[type] || "default"}
        >
          {type.toUpperCase()}
        </Tag>
      ),
    },
    {
      title: t("proxy.preview.nodeName"),
      dataIndex: "name",
      sorter: (left, right) => compareText(left.name, right.name),
      ellipsis: true,
      render: (name: string, record) => (
        <Tooltip
          title={
            record.filtered
              ? `${name}\n\n⚠️ ${t("proxy.preview.filteredBy", { rule: record.filteredBy })}`
              : name
          }
        >
          <span
            className={
              record.filtered ? "line-through text-slate-500" : undefined
            }
          >
            {name}
          </span>
        </Tooltip>
      ),
    },
    {
      title: t("proxy.preview.server"),
      dataIndex: "server",
      sorter: (left, right) => compareText(left.server, right.server),
      width: 200,
      ellipsis: true,
      render: (server: string, record) => (
        <Tooltip title={server}>
          <span
            className={`font-mono text-xs${record.filtered ? " text-slate-500" : ""}`}
          >
            {server}
          </span>
        </Tooltip>
      ),
    },
    {
      title: t("proxy.preview.port"),
      dataIndex: "port",
      sorter: (left, right) => left.port - right.port,
      width: 80,
      align: "center",
      render: (port: number, record) => (
        <span
          className={`font-mono text-xs${record.filtered ? " text-slate-500" : ""}`}
        >
          {port}
        </span>
      ),
    },
    {
      title: t("proxy.preview.transport"),
      dataIndex: "raw",
      width: 120,
      align: "center",
      render: (raw: Record<string, unknown>) => {
        const network = raw?.network as string | undefined;
        const tls = raw?.tls as boolean | undefined;
        return (
          <span className="text-xs">
            {network && <Tag>{network.toUpperCase()}</Tag>}
            {tls && <Tag color="green">TLS</Tag>}
            {!network && !tls && "-"}
          </span>
        );
      },
    },
    {
      title: t("proxy.preview.secret"),
      dataIndex: "raw",
      width: 180,
      render: (raw: Record<string, unknown>, record) => {
        const secret = (raw?.uuid || raw?.password || raw?.["auth-str"]) as
          | string
          | undefined;
        if (!secret) return <span className="text-slate-500">-</span>;
        return (
          <Tooltip title={t("proxy.preview.clickToCopyFull")}>
            <span
              className={`font-mono text-xs${record.filtered ? " text-slate-500" : ""}`}
            >
              {secret.length > 16
                ? `${secret.slice(0, 8)}...${secret.slice(-4)}`
                : secret}
            </span>
          </Tooltip>
        );
      },
    },
  ];

  const nodes = (data?.nodes ?? []) as ProxyNode[];

  // 统计节点数量
  const totalCount = nodes.length;
  const filteredCount = nodes.filter((n: ProxyNode) => n.filtered).length;
  const activeCount = totalCount - filteredCount;

  // 统计各协议的节点数量（仅有效节点）
  const typeCounts = nodes
    .filter((n: ProxyNode) => !n.filtered)
    .reduce<Record<string, number>>(
      (acc: Record<string, number>, node: ProxyNode) => {
        acc[node.type] = (acc[node.type] || 0) + 1;
        return acc;
      },
      {},
    );

  // 移动端卡片视图（匹配线上设计）

  return (
    <Modal
      title={
        <div className="flex items-center gap-2 flex-wrap">
          <EyeOutlined />
          <span>{t("proxy.preview.title")}</span>
          {!isMobile && <span>- {subscribeRemark}</span>}
        </div>
      }
      open={visible}
      onCancel={handleClose}
      footer={null}
      size={isMobile ? "full" : "almost-full"}
      styles={{
        body: {
          padding: isMobile ? "12px 8px" : "16px 0",
          overflowY: "auto",
          overflowX: "hidden",
        },
      }}
    >
      <Spin spinning={isLoading}>
        {!isLoading && nodes.length === 0 ? (
          <Empty description={t("proxy.preview.noNodes")} />
        ) : (
          <>
            {/* 移动端显示订阅名称 */}
            {isMobile && (
              <div className="text-sm text-slate-500 mb-2">
                {subscribeRemark}
              </div>
            )}

            {/* 统计信息 */}
            <div className="mb-4 px-2 md:px-4 flex items-center gap-2 flex-wrap text-sm">
              <span className="text-gray-500 dark:text-gray-400">
                共 {totalCount} 个节点，有效 {activeCount} 个
                {filteredCount > 0 && (
                  <span>
                    , 已过滤{" "}
                    <span className="text-orange-500">{filteredCount}</span>
                  </span>
                )}
              </span>
              {Object.entries(typeCounts).map(([type, count]) => (
                <Tag
                  key={type}
                  color={typeColorMap[type] || "default"}
                  className="!text-xs"
                >
                  {type.toUpperCase()}: {String(count)}
                </Tag>
              ))}
            </div>

            {isMobile ? (
              /* 移动端卡片列表 */
              <div className="flex flex-col gap-3 px-1">
                {nodes.map((node) => (
                  <MobileNodeCard
                    key={`${node.sourceIndex}-${node.name}-${node.server}-${node.port}`}
                    node={node}
                  />
                ))}
              </div>
            ) : (
              /* PC端表格 */
              <Table<ProxyNode>
                rowKey={(record, idx) =>
                  `${record.sourceIndex}-${record.server}-${record.port}-${idx}`
                }
                size="small"
                bordered
                columns={columns}
                dataSource={nodes}
                rowClassName={(record) => (record.filtered ? "opacity-60" : "")}
                expandable={{
                  expandedRowRender: (node) => {
                    const fields = protocolFields[node.type] || [];
                    const raw = node.raw || {};
                    const definedKeys = fields.map((f) => f.key);
                    const basicKeys = ["name", "type", "server", "port"];
                    const extraKeys = Object.keys(raw).filter(
                      (k) => !definedKeys.includes(k) && !basicKeys.includes(k),
                    );
                    const allFields = [
                      ...fields
                        .filter((f) => raw[f.key] !== undefined)
                        .map((f) => ({
                          key: f.key,
                          label: f.label,
                          value: raw[f.key],
                          sensitive: f.sensitive,
                        })),
                      ...extraKeys.map((k) => ({
                        key: k,
                        label: k,
                        value: raw[k],
                        sensitive: false,
                      })),
                    ];
                    if (allFields.length === 0) {
                      return (
                        <span className="text-slate-400 text-xs">
                          {t("proxy.preview.noDetails")}
                        </span>
                      );
                    }
                    const descItems: DescriptionsItem[] = allFields.map(
                      (f) => ({
                        key: f.key,
                        label: f.label,
                        children:
                          typeof f.value === "object" ? (
                            <pre className="m-0 text-xs font-mono whitespace-pre-wrap break-all">
                              {formatValue(f.value)}
                            </pre>
                          ) : (
                            <span className="font-mono break-all">
                              {formatValue(f.value)}
                            </span>
                          ),
                        span: typeof f.value === "object" ? 3 : undefined,
                      }),
                    );
                    return (
                      <Descriptions
                        bordered
                        size="small"
                        column={3}
                        items={descItems}
                      />
                    );
                  },
                }}
                pagination={{
                  defaultPageSize: 500,
                  showSizeChanger: true,
                  pageSizeOptions: [
                    "20",
                    "50",
                    "100",
                    "200",
                    "300",
                    "400",
                    "500",
                  ],
                  showTotal: (total) => `${total}`,
                }}
                scroll={{ x: 1000, y: "calc(100vh - 340px)" }}
              />
            )}
          </>
        )}
      </Spin>
    </Modal>
  );
});

ProxyPreviewModal.displayName = "ProxyPreviewModal";

export default ProxyPreviewModal;

export type { ProxyPreviewModalRef } from "./ProxyPreviewContent";
