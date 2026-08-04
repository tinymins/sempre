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
import { Copy } from "lucide-react";
import { forwardRef, useCallback, useImperativeHandle, useState } from "react";
import { useTranslation } from "react-i18next";
import { proxyApi } from "@/generated/rust-api";
import { useIsMobile } from "@/hooks";
import { message } from "@/lib/message";

export interface ProxyPreviewModalRef {
  open: (subscribeId: string, remark?: string | null) => void;
}

interface ProxyNode {
  name: string;
  type: string;
  server: string;
  port: number;
  sourceIndex: number;
  sourceUrl: string;
  raw: Record<string, unknown>;
  filtered?: boolean;
  filteredBy?: string;
}

/** 协议类型对应的颜色 */
const typeColorMap: Record<string, string> = {
  vmess: "blue",
  vless: "purple",
  ss: "green",
  trojan: "orange",
  hysteria2: "magenta",
  hysteria: "red",
  tuic: "cyan",
  socks5: "default",
  http: "default",
};

/** 不同协议的关键字段配置 */
const protocolFields: Record<
  string,
  { key: string; label: string; sensitive?: boolean }[]
> = {
  vmess: [
    { key: "uuid", label: "UUID", sensitive: true },
    { key: "alterId", label: "Alter ID" },
    { key: "cipher", label: "加密方式" },
    { key: "network", label: "传输协议" },
    { key: "tls", label: "TLS" },
    { key: "servername", label: "SNI" },
    { key: "ws-opts", label: "WebSocket 配置" },
    { key: "grpc-opts", label: "gRPC 配置" },
  ],
  vless: [
    { key: "uuid", label: "UUID", sensitive: true },
    { key: "flow", label: "Flow" },
    { key: "network", label: "传输协议" },
    { key: "tls", label: "TLS" },
    { key: "sni", label: "SNI" },
    { key: "client-fingerprint", label: "指纹" },
    { key: "reality-opts", label: "Reality 配置" },
    { key: "ws-opts", label: "WebSocket 配置" },
    { key: "grpc-opts", label: "gRPC 配置" },
  ],
  ss: [
    { key: "cipher", label: "加密方式" },
    { key: "password", label: "密码", sensitive: true },
    { key: "plugin", label: "插件" },
    { key: "plugin-opts", label: "插件配置" },
    { key: "udp", label: "UDP" },
  ],
  trojan: [
    { key: "password", label: "密码", sensitive: true },
    { key: "sni", label: "SNI" },
    { key: "alpn", label: "ALPN" },
    { key: "skip-cert-verify", label: "跳过证书验证" },
    { key: "client-fingerprint", label: "指纹" },
    { key: "network", label: "传输协议" },
    { key: "ws-opts", label: "WebSocket 配置" },
    { key: "grpc-opts", label: "gRPC 配置" },
  ],
  hysteria2: [
    { key: "password", label: "密码", sensitive: true },
    { key: "sni", label: "SNI" },
    { key: "obfs", label: "混淆类型" },
    { key: "obfs-password", label: "混淆密码", sensitive: true },
    { key: "alpn", label: "ALPN" },
    { key: "skip-cert-verify", label: "跳过证书验证" },
  ],
  hysteria: [
    { key: "auth-str", label: "认证字符串", sensitive: true },
    { key: "obfs", label: "混淆" },
    { key: "protocol", label: "协议" },
    { key: "up", label: "上行带宽" },
    { key: "down", label: "下行带宽" },
    { key: "sni", label: "SNI" },
    { key: "alpn", label: "ALPN" },
  ],
  tuic: [
    { key: "uuid", label: "UUID", sensitive: true },
    { key: "password", label: "密码", sensitive: true },
    { key: "congestion-controller", label: "拥塞控制" },
    { key: "udp-relay-mode", label: "UDP 中继模式" },
    { key: "sni", label: "SNI" },
    { key: "alpn", label: "ALPN" },
    { key: "reduce-rtt", label: "减少 RTT" },
  ],
};

/** 格式化值显示 */
const formatValue = (
  value: unknown,
  yesText = "Yes",
  noText = "No",
): string => {
  if (value === undefined || value === null) return "-";
  if (typeof value === "boolean") return value ? yesText : noText;
  if (typeof value === "object") return JSON.stringify(value, null, 2);
  return String(value);
};

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
  const MobileNodeCard = ({ node }: { node: ProxyNode }) => {
    const [expanded, setExpanded] = useState(false);
    const fields = protocolFields[node.type] || [];
    const raw = node.raw || {};
    const definedKeys = fields.map((f) => f.key);
    const basicKeys = ["name", "type", "server", "port"];
    const extraKeys = Object.keys(raw).filter(
      (k) => !definedKeys.includes(k) && !basicKeys.includes(k),
    );
    const hasDetails = fields.length > 0 || extraKeys.length > 0;

    const copyToClipboard = useCallback((text: string) => {
      navigator.clipboard.writeText(text).then(() => {
        message.success(t("proxy.preview.copied") || "已复制");
      });
    }, []);

    return (
      <div
        className={`rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden ${node.filtered ? "opacity-50" : ""}`}
      >
        {/* Header: name + protocol tag */}
        <div className="flex items-center justify-between gap-2 px-3 py-2.5 bg-gray-50 dark:bg-white/[0.04]">
          <span
            className={`font-medium text-sm truncate flex-1 ${node.filtered ? "line-through text-gray-400" : ""}`}
            title={node.name}
          >
            {node.name}
          </span>
          <Tag
            color={
              node.filtered ? "default" : typeColorMap[node.type] || "default"
            }
            className="!m-0 shrink-0"
          >
            {node.type.toUpperCase()}
          </Tag>
        </div>

        {/* Body: server / port / source */}
        <div className="px-3 py-2 space-y-1.5 text-sm">
          <div className="flex items-center justify-between">
            <span className="text-gray-500 dark:text-gray-400 shrink-0">
              {t("proxy.preview.server")}:
            </span>
            <div className="flex items-center gap-1.5">
              <span className="font-mono text-xs" title={node.server}>
                {node.server}
              </span>
              <Copy
                size={13}
                className="text-gray-400 hover:text-blue-500 cursor-pointer shrink-0"
                onClick={() => copyToClipboard(node.server)}
              />
            </div>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-gray-500 dark:text-gray-400 shrink-0">
              {t("proxy.preview.port")}:
            </span>
            <span className="font-mono text-xs">{node.port}</span>
          </div>
          {Boolean(node.raw?.network) && (
            <div className="flex items-center justify-between">
              <span className="text-gray-500 dark:text-gray-400 shrink-0">
                {t("proxy.preview.transport")}:
              </span>
              <span className="flex items-center gap-1">
                <Tag className="!m-0">
                  {String(node.raw.network).toUpperCase()}
                </Tag>
                {Boolean(node.raw?.tls) && (
                  <Tag color="green" className="!m-0">
                    TLS
                  </Tag>
                )}
              </span>
            </div>
          )}
          <div className="flex items-center justify-between">
            <span className="text-gray-500 dark:text-gray-400 shrink-0">
              {t("proxy.preview.source")}:
            </span>
            <Tag
              color={node.sourceIndex === 0 ? "default" : "blue"}
              className="!m-0"
            >
              {node.sourceIndex === 0
                ? t("proxy.preview.manual")
                : `#${node.sourceIndex}`}
            </Tag>
          </div>
          {node.filtered && node.filteredBy && (
            <div className="text-orange-500 text-xs mt-1">
              ⚠️ {t("proxy.preview.filteredBy", { rule: node.filteredBy })}
            </div>
          )}
        </div>

        {/* Expand/Collapse toggle */}
        {hasDetails && (
          <>
            <div className="border-t border-gray-200 dark:border-gray-700">
              <div
                className="text-center py-2 text-blue-500 text-sm cursor-pointer hover:bg-gray-50 dark:hover:bg-white/[0.03]"
                onClick={() => setExpanded(!expanded)}
              >
                {expanded
                  ? t("proxy.preview.collapse") || "收起"
                  : t("proxy.preview.expand") || "展开详情"}
              </div>
            </div>

            {/* Expanded protocol details */}
            {expanded && (
              <div className="border-t border-gray-200 dark:border-gray-700 px-3 py-2 space-y-1.5 text-sm">
                {fields.map((field) => {
                  const value = raw[field.key];
                  if (value === undefined) return null;
                  return (
                    <div
                      key={field.key}
                      className="flex justify-between items-start gap-2"
                    >
                      <span className="text-gray-500 dark:text-gray-400 shrink-0">
                        {field.label}:
                      </span>
                      {typeof value === "object" ? (
                        <pre className="m-0 text-xs font-mono bg-gray-100 dark:bg-gray-800 p-2 rounded max-w-[65%] overflow-x-auto whitespace-pre-wrap">
                          {formatValue(value)}
                        </pre>
                      ) : (
                        <div className="flex items-center gap-1.5 max-w-[65%]">
                          <span className="font-mono text-xs text-right break-all">
                            {field.sensitive &&
                            typeof value === "string" &&
                            value.length > 16
                              ? `${value.slice(0, 8)}...${value.slice(-8)}`
                              : formatValue(value)}
                          </span>
                          {field.sensitive && typeof value === "string" && (
                            <Copy
                              size={13}
                              className="text-gray-400 hover:text-blue-500 cursor-pointer shrink-0"
                              onClick={() => copyToClipboard(String(value))}
                            />
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}
                {extraKeys.map((key) => {
                  const value = raw[key];
                  return (
                    <div
                      key={key}
                      className="flex justify-between items-start gap-2"
                    >
                      <span className="text-gray-500 dark:text-gray-400 shrink-0">
                        {key}:
                      </span>
                      {typeof value === "object" ? (
                        <pre className="m-0 text-xs font-mono bg-gray-100 dark:bg-gray-800 p-2 rounded max-w-[65%] overflow-x-auto whitespace-pre-wrap">
                          {formatValue(value)}
                        </pre>
                      ) : (
                        <span className="font-mono text-xs text-right max-w-[65%] break-all">
                          {formatValue(value)}
                        </span>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </>
        )}
      </div>
    );
  };

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
