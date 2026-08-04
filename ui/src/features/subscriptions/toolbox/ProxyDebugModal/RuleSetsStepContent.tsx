import { Collapse, Tag } from "@acme/components";
import type { ProxyDebugStep } from "@acme/types";
import { useTranslation } from "react-i18next";

type RuleSetsStep = Extract<ProxyDebugStep, { type: "rule-sets" }>;
type RuleSetItem = RuleSetsStep["data"]["items"][number];

/** 规则内容代码块 */
const RuleCodeBlock = ({ rules }: { rules: string[] }) => (
  <pre className="!m-0 !p-3 !text-xs !bg-gray-50 dark:!bg-gray-900 !rounded-md !overflow-auto !whitespace-pre-wrap !break-all !font-mono max-h-[300px]">
    {rules.join("\n")}
  </pre>
);

/** 单个规则集项 */
const RuleSetItemContent = ({ item }: { item: RuleSetItem }) => {
  const { t } = useTranslation();

  if (item.status === "error") {
    return (
      <div className="flex flex-col gap-1">
        <div className="text-xs text-red-500 font-mono break-all">
          {item.error}
        </div>
        <div className="text-xs text-slate-400 break-all">{item.url}</div>
      </div>
    );
  }

  if (item.status === "skipped") {
    return (
      <div className="text-xs text-slate-500">
        {item.format === "binary"
          ? t("proxy.debug.ruleSetBuiltinHint")
          : "skipped"}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="text-xs text-slate-400 break-all">{item.url}</div>
      {item.effectiveUrl && item.effectiveUrl !== item.url && (
        <div className="text-xs text-slate-400 break-all">
          <span className="text-slate-500">
            {t("proxy.debug.ruleSetEffectiveUrl")}:{" "}
          </span>
          {item.effectiveUrl}
        </div>
      )}
      {item.sampleRules && item.sampleRules.length > 0 && (
        <RuleCodeBlock rules={item.sampleRules} />
      )}
    </div>
  );
};

/** 单个规则集 Collapse 头部 */
const RuleSetItemLabel = ({ item }: { item: RuleSetItem }) => {
  const { t } = useTranslation();

  return (
    <div className="flex gap-2 items-center flex-wrap">
      <span className="font-mono text-xs">{item.tag}</span>
      {item.builtin && (
        <Tag className="!text-xs">{t("proxy.debug.ruleSetBuiltinTag")}</Tag>
      )}
      {item.status === "ok" && (
        <Tag color="green" className="!text-xs">
          {item.ruleCount} {t("proxy.debug.ruleSetRulesUnit")}
        </Tag>
      )}
      {item.status === "skipped" && (
        <Tag className="!text-xs">
          {item.format === "binary" ? "binary" : "skipped"}
        </Tag>
      )}
      {item.status === "error" && (
        <Tag color="error" className="!text-xs">
          {item.error ?? t("proxy.debug.error")}
        </Tag>
      )}
    </div>
  );
};

/** 按分组聚合规则集 */
function groupByGroup(items: RuleSetItem[]) {
  const groups: Map<string, RuleSetItem[]> = new Map();
  for (const item of items) {
    const key = item.group;
    if (!groups.has(key)) {
      groups.set(key, []);
    }
    groups.get(key)?.push(item);
  }
  return groups;
}

/** 规则集调试步骤 */
export const RuleSetsStepContent = ({ step }: { step: RuleSetsStep }) => {
  const { t } = useTranslation();
  const { data } = step;
  const grouped = groupByGroup(data.items);

  return (
    <div className="flex flex-col gap-2">
      <Collapse
        size="small"
        items={[
          {
            key: "rule-sets-detail",
            label: (
              <div className="flex gap-2 items-center flex-wrap">
                <span>{t("proxy.debug.ruleSets")}</span>
                <Tag className="!text-xs">
                  {data.totalCount} {t("proxy.debug.ruleSetSetsUnit")}
                </Tag>
                <Tag color="cyan" className="!text-xs">
                  {data.totalRules} {t("proxy.debug.ruleSetRulesUnit")}
                </Tag>
                {data.errorCount > 0 && (
                  <Tag color="error" className="!text-xs">
                    {data.errorCount} {t("proxy.debug.error")}
                  </Tag>
                )}
              </div>
            ),
            children: (
              <Collapse
                size="small"
                items={Array.from(grouped.entries()).map(
                  ([groupName, items]) => {
                    const groupRuleCount = items.reduce(
                      (sum, item) => sum + item.ruleCount,
                      0,
                    );
                    const groupErrorCount = items.filter(
                      (item) => item.status === "error",
                    ).length;

                    return {
                      key: groupName,
                      label: (
                        <div className="flex gap-2 items-center flex-wrap">
                          <span>{groupName}</span>
                          <Tag className="!text-xs">
                            {items.length} {t("proxy.debug.ruleSetSetsUnit")}
                          </Tag>
                          <Tag color="cyan" className="!text-xs">
                            {groupRuleCount} {t("proxy.debug.ruleSetRulesUnit")}
                          </Tag>
                          {groupErrorCount > 0 && (
                            <Tag color="error" className="!text-xs">
                              {groupErrorCount} {t("proxy.debug.error")}
                            </Tag>
                          )}
                        </div>
                      ),
                      children: (
                        <Collapse
                          size="small"
                          items={items.map((item) => ({
                            key: item.tag,
                            label: <RuleSetItemLabel item={item} />,
                            children: <RuleSetItemContent item={item} />,
                          }))}
                        />
                      ),
                    };
                  },
                )}
              />
            ),
          },
        ]}
      />
    </div>
  );
};
