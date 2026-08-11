import {
  AimOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
  Collapse,
  Descriptions,
  ShieldCheckOutlined,
  Tag,
  Tooltip,
} from "@acme/components";
import type { ProxyDebugStep } from "@acme/types";
import { useTranslation } from "react-i18next";
import { SmartCodeBlock } from "./DebugStepCommon";

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
