import {
  AimOutlined,
  ArrowLeftOutlined,
  AutoComplete,
  Button,
  Empty,
  Modal,
  Spin,
  Tag,
} from "@acme/components";
import type { ProxyDebugFormat, ProxyNodeTraceStep } from "@acme/types";
import {
  forwardRef,
  useCallback,
  useImperativeHandle,
  useMemo,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { proxyApi } from "@/generated/rust-api";
import { useIsMobile } from "@/hooks";
import { TraceStepsContent } from "./NodeTraceSteps";

export interface NodeTraceModalRef {
  open: (nodeName?: string) => void;
}

interface NodeTraceModalProps {
  subscribeId: string;
  format: ProxyDebugFormat;
  allNodeNames: { name: string; filtered: boolean }[];
  nodeWarnings?: Set<string>;
  nodeIgnored?: Set<string>;
}

const NodeTraceModal = forwardRef<NodeTraceModalRef, NodeTraceModalProps>(
  ({ subscribeId, format, allNodeNames, nodeWarnings, nodeIgnored }, ref) => {
    const { t } = useTranslation();
    const isMobile = useIsMobile();
    const [visible, setVisible] = useState(false);
    const [tracingNodeName, setTracingNodeName] = useState<string | null>(null);
    const [searchValue, setSearchValue] = useState("");

    useImperativeHandle(ref, () => ({
      open: (nodeName?: string) => {
        if (nodeName) {
          setTracingNodeName(nodeName);
          setSearchValue(nodeName);
        } else {
          setTracingNodeName(null);
          setSearchValue("");
        }
        setVisible(true);
      },
    }));

    const { data, isLoading, error } = proxyApi.traceNode.useQuery(
      tracingNodeName
        ? { id: subscribeId, format, nodeName: tracingNodeName }
        : undefined,
      { enabled: !!tracingNodeName },
    );

    const handleTraceNode = useCallback((nodeName: string) => {
      setTracingNodeName(nodeName);
      setSearchValue(nodeName);
    }, []);

    const handleClose = () => {
      setVisible(false);
    };

    const autoCompleteOptions = useMemo(() => {
      const query = searchValue.toLowerCase();
      return allNodeNames
        .filter((n) => !query || n.name.toLowerCase().includes(query))
        .slice(0, 50)
        .map((n) => ({
          value: n.name,
          label: (
            <div className="flex items-center justify-between">
              <span
                className={`text-xs truncate flex-1 ${n.filtered ? "text-slate-500 line-through" : ""}`}
              >
                {n.name}
              </span>
              {n.filtered && (
                <Tag color="orange" className="!text-xs ml-1 shrink-0">
                  {t("proxy.debug.traceFilteredLabel")}
                </Tag>
              )}
            </div>
          ),
        }));
    }, [allNodeNames, searchValue, t]);

    const filterStep = data?.steps?.find((s) => s.type === "filter") as
      | Extract<ProxyNodeTraceStep, { type: "filter" }>
      | undefined;
    const isFiltered = filterStep?.type === "filter" && !filterStep.data.passed;

    return (
      <Modal
        title={
          <div className="flex items-center gap-3">
            <Button
              variant="text"
              size="small"
              icon={<ArrowLeftOutlined />}
              onClick={handleClose}
            />
            <AimOutlined className="text-blue-500" />
            <span>{t("proxy.debug.traceTitle")}</span>
            {tracingNodeName && (
              <Tag
                color={
                  nodeWarnings?.has(tracingNodeName)
                    ? "gold"
                    : nodeIgnored?.has(tracingNodeName)
                      ? "blue"
                      : "green"
                }
              >
                {tracingNodeName}
              </Tag>
            )}
            {isFiltered && (
              <Tag color="orange">{t("proxy.debug.traceFilteredLabel")}</Tag>
            )}
          </div>
        }
        open={visible}
        onCancel={handleClose}
        footer={null}
        size={isMobile ? "full" : "almost-full"}
        destroyOnClose={false}
      >
        {/* 搜索栏 */}
        <div className="flex items-center gap-3 mb-4 flex-wrap">
          <AutoComplete
            value={searchValue}
            options={autoCompleteOptions}
            onSearch={(value: string) => {
              setSearchValue(value);
              if (!value) {
                setTracingNodeName(null);
              }
            }}
            onSelect={(value: string) => handleTraceNode(value)}
            placeholder={t("proxy.debug.traceSearchPlaceholder")}
            className="flex-1 min-w-[200px] max-w-[500px]"
            allowClear
          />
          <span className="text-slate-500 text-xs">
            {t("proxy.debug.traceNodeList")}: {allNodeNames.length}
          </span>
        </div>

        {/* 内容区 */}
        {!tracingNodeName && (
          <div className="text-center py-12">
            <Empty description={t("proxy.debug.traceSelectNode")} />
          </div>
        )}

        {tracingNodeName && isLoading && (
          <div className="flex items-center justify-center py-12">
            <Spin />
            <span className="text-slate-500 ml-2">
              {t("proxy.debug.traceLoading")}
            </span>
          </div>
        )}

        {tracingNodeName && error && (
          <div className="mb-4 p-3 rounded-lg border border-red-200 bg-red-50 dark:border-red-800 dark:bg-red-900/20">
            <div className="font-semibold text-red-600 dark:text-red-400">
              {t("proxy.debug.error")}
            </div>
            <div className="text-sm text-red-500 dark:text-red-400 mt-1">
              {error.message}
            </div>
          </div>
        )}

        {tracingNodeName && data && data.steps.length === 0 && !isLoading && (
          <Empty description={t("proxy.debug.traceNodeNotFound")} />
        )}

        {tracingNodeName && data && data.steps.length > 0 && (
          <TraceStepsContent
            data={data as { nodeName: string; steps: ProxyNodeTraceStep[] }}
            format={format}
          />
        )}
      </Modal>
    );
  },
);

NodeTraceModal.displayName = "NodeTraceModal";

export default NodeTraceModal;
