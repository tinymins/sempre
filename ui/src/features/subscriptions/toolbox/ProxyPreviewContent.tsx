import { Tag } from "@acme/components";
import { Copy } from "lucide-react";
import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { message } from "@/lib/message";

export interface ProxyPreviewModalRef {
  open: (subscribeId: string, remark?: string | null) => void;
}

export interface ProxyNode {
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
export const typeColorMap: Record<string, string> = {
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
export const protocolFields: Record<
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
export const formatValue = (
  value: unknown,
  yesText = "Yes",
  noText = "No",
): string => {
  if (value === undefined || value === null) return "-";
  if (typeof value === "boolean") return value ? yesText : noText;
  if (typeof value === "object") return JSON.stringify(value, null, 2);
  return String(value);
};


export const MobileNodeCard = ({ node }: { node: ProxyNode }) => {
  const { t } = useTranslation();
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
    }, [t]);

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
