import {
	Alert,
  Button,
  Checkbox,
  Form,
  Input,
  Modal,
  Select,
  Tabs,
  Tag,
  TextArea,
} from "@acme/components";
import { forwardRef, useImperativeHandle } from "react";
import { useTranslation } from "react-i18next";
import type { CustomNode, SubscriptionSource } from "@/lib/types";
import PrivateAccessEditor from "./PrivateAccessEditor";
import SubscribeItemsEditor from "./SubscribeItemsEditor";

import { ConfigFieldEditor, DnsConfigEditorField, JsoncEditor, NodeFilterField } from "./ProxySubscribeFields";
import { type Props, type SaveFeedback, recommendedEditorDefaults } from "./ProxySubscribeModel";
import { ProxyRuntimeFields } from "./ProxyRuntimeFields";
import { useProxySubscribeEditor } from "./useProxySubscribeEditor";

export type { ProxySubscribeSaveState } from "./ProxySubscribeModel";

export interface ProxySubscribeEditorRef {
  saveNow: () => void;
}

const ProxySubscribeEditor = forwardRef<ProxySubscribeEditorRef, Props>((props, ref) => {
  const {
    t,
    setActiveTab,
    visibleActiveTab,
    localizedTabs,
    profileFeedback,
    scheduleFeedback,
    configurationContext,
    form,
    queueAutosave,
    saveNow,
    features,
    scheduleInterval,
    setScheduleInterval,
    queueScheduleSave,
    autoRestart,
    setAutoRestart,
    rawSources,
    rawSourcesRef,
    setRawSources,
    defaults,
    CONFIG_FIELDS,
    supportsLocalProxy,
    supportsTransparent,
    supportsManagement,
    transparentMode,
    tunInterfaceMode,
    networkInventory,
    customNodes,
    manualServerCount,
    openManualServersEditor,
    diagnostics,
    sourceDebug,
    manualServersEditorOpen,
    setManualServersEditorOpen,
    saveManualServers,
    manualServersDraft,
    setManualServersDraft,
    manualServersError,
  } = useProxySubscribeEditor(props);
	const recommendedDefaults = recommendedEditorDefaults(defaults, configurationContext);

  useImperativeHandle(ref, () => ({ saveNow }), [saveNow]);

    return (
      <div className="min-h-0 rounded-lg border border-black/[0.08] bg-white/50 p-4 dark:border-white/[0.08] dark:bg-white/[0.02]">
          <div className="mb-3 shrink-0 overflow-x-auto pb-1">
            <Tabs
              className="min-w-[920px]"
              type="segment"
              activeKey={visibleActiveTab}
              onChange={(key) => setActiveTab(key)}
              items={localizedTabs.map((tab) => ({
                key: tab.value,
                label: tab.label,
              }))}
            />
          </div>
          <SaveStatus profile={profileFeedback} schedule={scheduleFeedback} />
					{configurationContext.target && configurationContext.running && configurationContext.target.core !== configurationContext.running.core ? (
						<Alert
							type="warning"
							showIcon
							message={t("proxy.form.coreTransition", { target: configurationContext.target.core, running: configurationContext.running.core })}
						/>
					) : null}

          <fieldset disabled={props.readOnly} className={props.readOnly ? "pointer-events-none m-0 min-w-0 border-0 p-0 opacity-80" : "m-0 min-w-0 border-0 p-0"}>
          <Form form={form} layout="vertical" autoComplete="off" onValuesChange={props.readOnly ? undefined : queueAutosave}>
            {/* 基础信息 */}
            <div style={{ display: visibleActiveTab === "basic" ? "block" : "none" }}>
              <Form.Item label={t("proxy.form.remark")} name="remark">
                <TextArea
                  rows={3}
                  placeholder={t("proxy.form.remarkPlaceholder")}
                />
              </Form.Item>
              {features.has("logging.level") ? <Form.Item
                label={t("proxy.form.logLevel")}
                name="logLevel"
                tooltip={t("proxy.form.logLevelTip")}
              >
                <Select
                  options={[
                    {
                      value: "off",
                      label: t("proxy.form.logLevelOff"),
                    },
                    {
                      value: "error",
                      label: t("proxy.form.logLevelError"),
                    },
                    {
                      value: "warn",
                      label: t("proxy.form.logLevelWarn"),
                    },
                    {
                      value: "info",
                      label: t("proxy.form.logLevelInfo"),
                    },
                    {
                      value: "debug",
                      label: t("proxy.form.logLevelDebug"),
                    },
                  ]}
                />
              </Form.Item> : null}
              <div className="grid gap-4 border-t border-gray-200 pt-4 dark:border-gray-700 md:grid-cols-2">
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>{t("proxy.form.updateSchedule")}</span>
                  <Input
                    value={scheduleInterval}
                    onChange={(event) => {
                      const interval = event.target.value;
                      setScheduleInterval(interval);
                      queueScheduleSave({ interval });
                    }}
                  />
                </label>
                {props.showAutoRestart !== false ? <label className="flex min-h-9 items-center gap-2 self-end rounded-md border border-[var(--border)] px-3 text-sm">
                  <Checkbox
                    checked={autoRestart}
                    onChange={(event) => {
                      const value = event.target.checked;
                      setAutoRestart(value);
                      queueScheduleSave({ auto_restart: value }, true);
                    }}
                  />
                  <span>{t("proxy.form.restartAfterScheduledUpdates")}</span>
                </label> : null}
              </div>
            </div>

            {/* 订阅源 */}
            <div
              style={{
                display: visibleActiveTab === "subscribeUrl" ? "block" : "none",
              }}
            >
              <Form.Item
                label={t("proxy.form.subscribeUrlLabel")}
                name="subscribeItems"
              >
                <SubscribeItemsEditor allowDebug={sourceDebug} />
              </Form.Item>

              {rawSources.map((source, index) => (
                <div key={source.id} className="mt-3 rounded-lg border border-gray-200 p-3 dark:border-gray-700">
                  <div className="mb-2 flex items-center gap-2">
                    <Tag>RAW</Tag>
                    <input
                      className="min-w-0 flex-1 bg-transparent text-sm outline-none"
                      value={source.remark ?? ""}
                      placeholder={t("proxy.form.subscribeItemRemark")}
                    onChange={(event) => {
                      const next = rawSourcesRef.current.map((item, position) => position === index ? { ...item, remark: event.target.value } : item);
                      rawSourcesRef.current = next;
                      setRawSources(next);
                      queueAutosave();
                    }}
                  />
                    <Button variant="text" size="small" danger onClick={() => {
                      const next = rawSourcesRef.current.filter((_, position) => position !== index);
                      rawSourcesRef.current = next;
                      setRawSources(next);
                      queueAutosave();
                    }}>
                      {t("proxy.actions.delete")}
                    </Button>
                  </div>
                  <TextArea
                    rows={8}
                    value={source.content ?? ""}
                    placeholder="proxies:"
                    onChange={(event) => {
                      const next = rawSourcesRef.current.map((item, position) => position === index ? { ...item, content: event.target.value } : item);
                      rawSourcesRef.current = next;
                      setRawSources(next);
                      queueAutosave();
                    }}
                  />
                </div>
              ))}
              <Button
                className="!mt-3"
                variant="dashed"
                block
                onClick={() => {
                  const next: SubscriptionSource[] = [...rawSourcesRef.current, { id: crypto.randomUUID(), type: "raw", enabled: true, content: "", remark: "" }];
                  rawSourcesRef.current = next;
                  setRawSources(next);
                  queueAutosave();
                }}
              >
                {t("proxy.form.addRawSource")}
              </Button>

              {/* 节点过滤器 */}
              <div className="mt-4 pt-4 border-t border-gray-200 dark:border-zinc-700">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-sm font-medium">
                    {t("proxy.form.nodeFilterLabel")}
                  </span>
                  <Form.Item
                    name="useSystemFilter"
                    valuePropName="checked"
                    noStyle
                  >
                    <Checkbox>{t("proxy.form.useSystemConfig")}</Checkbox>
                  </Form.Item>
                </div>
                <NodeFilterField
                  form={form}
                  defaultValue={recommendedDefaults.filter || "[]"}
                />
              </div>
            </div>

            {/* 规则列表 / 分组 / 过滤器 / 自定义配置 */}
            {CONFIG_FIELDS.map(
              ({ field, useSystemField, tab, labelKey, placeholderKey }) => (
                <div key={tab} className={visibleActiveTab === tab ? "" : "hidden"}>
                  <div className="flex items-center justify-between mb-2">
                    <span>{t(labelKey)}</span>
                    <Form.Item
                      name={useSystemField}
                      valuePropName="checked"
                      noStyle
                    >
                      <Checkbox>{t("proxy.form.useSystemConfig")}</Checkbox>
                    </Form.Item>
                  </div>
                  <Form.Item
                    name={field}
                    dependencies={[useSystemField]}
                    noStyle
                  >
                    <ConfigFieldEditor
                      form={form}
                      useSystemField={useSystemField}
                      defaultValue={field === "ruleList" ? recommendedDefaults.rule_list : field === "group" ? recommendedDefaults.group : recommendedDefaults.custom_config}
                      placeholder={t(placeholderKey)}
                    />
                  </Form.Item>
                </div>
              ),
            )}

            {/* DNS 配置 */}
            <div className={visibleActiveTab === "dnsConfig" ? "" : "hidden"}>
              <div className="flex items-center justify-between mb-2">
                <span>{t("proxy.form.dnsConfigLabel")}</span>
                <Form.Item
                  name="useSystemDnsConfig"
                  valuePropName="checked"
                  noStyle
                >
                  <Checkbox>{t("proxy.form.useSystemConfig")}</Checkbox>
                </Form.Item>
              </div>
              <Form.Item
                name="dnsConfig"
                dependencies={["useSystemDnsConfig"]}
                noStyle
              >
                <DnsConfigEditorField
                  form={form}
                  defaultValue={recommendedDefaults.dns_config}
								configurationContext={configurationContext}
								networkInventory={networkInventory}
                />
              </Form.Item>
            </div>

            {/* 内网访问配置 */}
            <div
              className={visibleActiveTab === "privateAccessConfig" ? "" : "hidden"}
            >
              <Form.Item
                label={t("proxy.form.privateAccessConfigLabel")}
                name="privateAccessConfig"
              >
                <PrivateAccessEditor />
              </Form.Item>
            </div>

            {visibleActiveTab === "runtime" ? (
              <ProxyRuntimeFields
                supportsLocalProxy={supportsLocalProxy}
                supportsTransparent={supportsTransparent}
                supportsManagement={supportsManagement}
                features={features}
                form={form}
                transparentMode={transparentMode}
                tunInterfaceMode={tunInterfaceMode}
                networkInventory={networkInventory}
              />
            ) : null}
            {/* 额外服务器 */}
            <div className={visibleActiveTab === "servers" ? "" : "hidden"}>
              <Form.Item
                label={t("proxy.form.assignedCustomNodes")}
                name="selectedCustomNodeIds"
                tooltip={t("proxy.form.assignedCustomNodesTip")}
              >
                <Select
                  mode="multiple"
                  showSearch
                  placeholder={t("proxy.form.assignedCustomNodesPlaceholder")}
                  options={customNodes.map(
                    (node: CustomNode) => ({
                      value: node.id,
                      label: `${node.name} · ${String(node.proxy.type || "")} · ${String(node.proxy.server || "")}:${String(node.proxy.port || "")}`,
                      tagLabel: node.name,
                    }),
                  )}
                />
              </Form.Item>
              <Form.Item name="servers" hidden>
                <TextArea />
              </Form.Item>
              <div className="mt-6 flex items-center gap-2">
                <span className="text-sm text-[var(--text-secondary)]">
                  {t("proxy.form.serversLabel")}
                </span>
                <Tag>{manualServerCount}</Tag>
                <Button
                  variant="link"
                  size="small"
                  onClick={openManualServersEditor}
                >
                  {t("proxy.actions.edit")}
                </Button>
              </div>
            </div>
          </Form>
          </fieldset>
          {visibleActiveTab === "diagnostics" ? <div>{diagnostics}</div> : null}
        <Modal
          title={t("proxy.form.serversLabel")}
          open={manualServersEditorOpen}
          onCancel={() => setManualServersEditorOpen(false)}
          onOk={saveManualServers}
          okText={t("common.save")}
          cancelText={t("common.cancel")}
          width={900}
          destroyOnClose
        >
          <JsoncEditor
            value={manualServersDraft}
            onChange={setManualServersDraft}
            placeholder={t("proxy.form.serversPlaceholder")}
          />
          {manualServersError ? <p role="alert" className="mt-2 text-sm text-red-500">{manualServersError}</p> : null}
        </Modal>
      </div>
    );
});

ProxySubscribeEditor.displayName = "ProxySubscribeEditor";

function SaveStatus({ profile, schedule }: { profile: SaveFeedback; schedule: SaveFeedback }) {
  const { t } = useTranslation();
  const feedback = profile.state === "error" ? profile
    : schedule.state === "error" ? schedule
      : profile.state === "saving" || schedule.state === "saving" ? { state: "saving" as const }
        : profile.state === "waiting" || schedule.state === "waiting" ? { state: "waiting" as const }
          : profile.state === "saved" || schedule.state === "saved" ? { state: "saved" as const }
            : { state: "idle" as const };
  const label = feedback.state === "waiting" ? t("proxy.autosave.waiting")
    : feedback.state === "saving" ? t("proxy.autosave.saving")
      : feedback.state === "saved" ? t("proxy.autosave.saved")
        : feedback.message || "";
  return (
    <div className="mb-4 min-h-6 border-b border-gray-200 pb-3 text-sm dark:border-gray-700">
      {label ? <p role={feedback.state === "error" ? "alert" : "status"} className={feedback.state === "error" ? "break-words text-red-500" : "text-[var(--text-secondary)]"}>{label}</p> : null}
    </div>
  );
}

export default ProxySubscribeEditor;
