import {
  Button,
  Checkbox,
  type FormFieldValues,
  Form,
  Modal,
  Select,
  Tabs,
  Tag,
  TextArea,
} from "@acme/components";
import type { SubscribeItem } from "@acme/types";
import Editor, { loader, type Monaco } from "@monaco-editor/react";
import { parse as parseJsonc, type ParseError } from "jsonc-parser";
import * as monacoRuntime from "monaco-editor";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { message } from "@/lib/message";
import type { CustomNode, SubscriptionEditorConfig, SubscriptionProfile, SubscriptionSource } from "@/lib/types";
import DnsConfigEditor from "./DnsConfigEditor";
import PrivateAccessEditor from "./PrivateAccessEditor";
import SubscribeItemsEditor from "./SubscribeItemsEditor";
import TagListEditor from "./TagListEditor";

// Use the bundled Monaco runtime so the local editor does not depend on a CDN.
loader.config({ monaco: monacoRuntime });

// JSONC 编辑器组件，支持 // 和 /* */ 注释
interface JsoncEditorProps {
  value?: string;
  onChange?: (value: string) => void;
  placeholder?: string;
  readOnly?: boolean;
}

const JsoncEditor = ({ value, onChange, readOnly }: JsoncEditorProps) => {
  return (
    <div
      className={`border rounded overflow-hidden ${
        readOnly
          ? "border-gray-500 dark:border-gray-500 opacity-60"
          : "border-gray-300 dark:border-gray-600"
      }`}
    >
      <Editor
        height="calc(100vh - 280px)"
        language="json"
        value={value || ""}
        theme="vs-dark"
        onChange={(val: string | undefined) => {
          if (!readOnly) onChange?.(val || "");
        }}
        options={{
          automaticLayout: true,
          selectOnLineNumbers: true,
          fontSize: 14,
          fontFamily: "Menlo, Monaco, 'Courier New', monospace",
          wordWrap: "on",
          renderControlCharacters: true,
          renderWhitespace: "all",
          scrollBeyondLastLine: false,
          minimap: { enabled: false },
          tabSize: 2,
          readOnly: readOnly ?? false,
        }}
        beforeMount={(monaco: Monaco) => {
          // 配置 JSON 语言允许注释和尾随逗号
          monaco.languages.json.jsonDefaults.setDiagnosticsOptions({
            validate: true,
            allowComments: true,
            trailingCommas: "ignore",
          });
          monaco.editor.defineTheme("vs-dark", {
            base: "vs-dark",
            inherit: true,
            rules: [],
            colors: {
              "editor.background": "#141414",
            },
          });
        }}
      />
    </div>
  );
};

/**
 * 配置字段编辑器：根据 useSystem checkbox 状态切换只读/可编辑模式。
 * 勾选时显示系统默认值（只读），取消勾选显示用户自定义值（可编辑）。
 */
interface ConfigFieldEditorProps {
  value?: string;
  onChange?: (value: string) => void;
  form: ReturnType<typeof Form.useForm>[0];
  useSystemField: string;
  defaultValue: string;
  placeholder?: string;
}

const ConfigFieldEditor = ({
  value,
  onChange,
  form,
  useSystemField,
  defaultValue,
  placeholder,
}: ConfigFieldEditorProps) => {
  const useSystem = Form.useWatch(useSystemField, form);

  if (useSystem) {
    // 独立的只读编辑器，不连接 form 的 onChange，确保表单值不被覆盖
    return (
      <JsoncEditor
        key="system-default"
        value={defaultValue}
        readOnly
        placeholder={placeholder}
      />
    );
  }

  // 可编辑编辑器，连接 form
  return (
    <JsoncEditor
      key="user-custom"
      value={value}
      onChange={onChange}
      placeholder={placeholder}
    />
  );
};

/**
 * DNS 配置字段编辑器：根据 useSystemDnsConfig 切换只读/可编辑模式。
 * 使用 DnsConfigEditor 组件而非纯 JSONC 编辑器。
 */
interface DnsConfigEditorFieldProps {
  value?: string;
  onChange?: (value: string) => void;
  form: ReturnType<typeof Form.useForm>[0];
  defaultValue: string;
}

const DnsConfigEditorField = ({
  value,
  onChange,
  form,
  defaultValue,
}: DnsConfigEditorFieldProps) => {
  const useSystem = Form.useWatch("useSystemDnsConfig", form);

  if (useSystem) {
    return (
      <DnsConfigEditor key="system-default" value={defaultValue} readOnly />
    );
  }

  return (
    <DnsConfigEditor key="user-custom" value={value} onChange={onChange} />
  );
};

/**
 * Node filter field: bridges JSON string ↔ string[] for TagListEditor.
 * Uses useSystemFilter to toggle between system default (read-only) and user custom.
 */
interface NodeFilterFieldProps {
  form: ReturnType<typeof Form.useForm>[0];
  defaultValue: string;
}

const NodeFilterField = ({ form, defaultValue }: NodeFilterFieldProps) => {
  const { t } = useTranslation();
  const useSystem = Form.useWatch("useSystemFilter", form);

  const parseFilterJson = (json: string): string[] => {
    try {
      const parsed = parseJsonc(json);
      return Array.isArray(parsed)
        ? parsed.filter((s): s is string => typeof s === "string")
        : [];
    } catch {
      return [];
    }
  };

  if (useSystem) {
    return <TagListEditor value={parseFilterJson(defaultValue)} readOnly />;
  }

  return (
    <Form.Item name="filter" noStyle>
      <NodeFilterTagAdapter
        placeholder={t("proxy.form.nodeFilterAddPlaceholder")}
      />
    </Form.Item>
  );
};

/**
 * Adapter: converts Form's string value ↔ TagListEditor's string[] value.
 */
interface NodeFilterTagAdapterProps {
  value?: string;
  onChange?: (value: string) => void;
  placeholder?: string;
}

const NodeFilterTagAdapter = ({
  value,
  onChange,
  placeholder,
}: NodeFilterTagAdapterProps) => {
  const parseFilterJson = (json: string): string[] => {
    try {
      const parsed = parseJsonc(json);
      return Array.isArray(parsed)
        ? parsed.filter((s): s is string => typeof s === "string")
        : [];
    } catch {
      return [];
    }
  };

  const tags = parseFilterJson(value ?? "");

  const handleChange = (newTags: string[]) => {
    onChange?.(JSON.stringify(newTags));
  };

  return (
    <TagListEditor
      value={tags}
      onChange={handleChange}
      placeholder={placeholder}
    />
  );
};

interface Props {
  profile: SubscriptionProfile;
  defaults: SubscriptionEditorConfig;
  customNodes: CustomNode[];
  saving: boolean;
  onSave: (profile: SubscriptionProfile) => Promise<void> | void;
  onCancel: () => void;
}

const TABS = [
  { label: "basic", value: "basic" },
  { label: "subscribeUrl", value: "subscribeUrl" },
  { label: "ruleList", value: "ruleList" },
  { label: "group", value: "group" },
  { label: "customConfig", value: "customConfig" },
  { label: "dnsConfig", value: "dnsConfig" },
  { label: "privateAccessConfig", value: "privateAccessConfig" },
  { label: "servers", value: "servers" },
];

function profileFormValues(profile: SubscriptionProfile): FormFieldValues {
  const items: SubscribeItem[] = profile.sources
    .filter((source) => source.type === "url")
    .map((source) => ({
      id: source.id,
      enabled: source.enabled,
      url: source.url ?? "",
      prefix: source.prefix ?? "",
      remark: source.remark ?? "",
      cacheTtlMinutes: source.cache_ttl_minutes,
      fetchUa: source.user_agent || undefined,
      fetchMode: source.fetch_mode ?? "auto",
    }));
  return {
    remark: profile.remark ?? "",
    logLevel: profile.log_level ?? "info",
    subscribeItems:
      items.length > 0
        ? items
        : [
            {
              enabled: true,
              url: "",
              prefix: "",
              remark: "",
              fetchMode: "auto",
            },
          ],
    ruleList: profile.editor.rule_list ?? "",
    useSystemRuleList: profile.use_system_rules,
    group: profile.editor.group ?? "",
    useSystemGroup: profile.use_system_groups,
    filter: profile.editor.filter ?? "",
    useSystemFilter: profile.use_system_filters,
    customConfig: profile.editor.custom_config ?? "",
    useSystemCustomConfig: profile.use_system_custom_config,
    dnsConfig: profile.editor.dns_config ?? "",
    useSystemDnsConfig: profile.use_system_dns,
    privateAccessConfig: profile.editor.private_access_config ?? "",
    servers: profile.editor.servers || "[]",
    selectedCustomNodeIds: profile.custom_node_ids ?? [],
  };
}

function isValidJsonc(value: string) {
  const errors: ParseError[] = [];
  parseJsonc(value, errors, { allowTrailingComma: true });
  return errors.length === 0;
}

const ProxySubscribeModal = ({
  profile,
  defaults,
  customNodes,
  saving,
  onSave,
  onCancel,
}: Props) => {
    const { t } = useTranslation();
    const [activeTab, setActiveTab] = useState("basic");
    const [manualServersEditorOpen, setManualServersEditorOpen] =
      useState(false);
    const [manualServersDraft, setManualServersDraft] = useState("");
    const [rawSources, setRawSources] = useState<SubscriptionSource[]>(() =>
      profile.sources.filter((source) => source.type === "raw"),
    );
    const [form] = Form.useForm(profileFormValues(profile));
    const manualServers = Form.useWatch("servers", form) as string | undefined;

    // 获取 tabs 的本地化标签
    const localizedTabs = TABS.map((tab) => ({
      ...tab,
      label: t(`proxy.tabs.${tab.label}`),
    }));

    const handleSubmit = async () => {
      try {
        const values = await form.validateFields();

        // 验证 JSONC 格式是否正确
        const validateJsonc = (field: string) => {
          if (!values[field]) return true;
          if (isValidJsonc(values[field])) return true;
          message.error(`${field} ${t("proxy.form.jsonFormatError")}`);
          return false;
        };

        // 验证所有 JSONC 字段
        const fields = [
          "ruleList",
          "group",
          "customConfig",
          "dnsConfig",
          "privateAccessConfig",
          "servers",
        ];
        for (const field of fields) {
          if (!validateJsonc(field)) {
            throw new Error(`${field} ${t("proxy.form.jsonFormatError")}`);
          }
        }

        // 过滤空白的 subscribeItems，清空旧字段
        const cleanedItems = (
          (values.subscribeItems as SubscribeItem[]) || []
        ).filter((item: SubscribeItem) => item.url?.trim());

        const sources: SubscriptionSource[] = cleanedItems.map((item: SubscribeItem) => ({
          id: item.id || crypto.randomUUID(),
          type: "url",
          enabled: item.enabled,
          url: item.url.trim(),
          prefix: item.prefix || undefined,
          remark: item.remark || undefined,
          user_agent: item.fetchUa || "clash.meta",
          fetch_mode: item.fetchMode ?? "auto",
          cache_ttl_minutes: item.cacheTtlMinutes,
        }));
        await onSave({
          ...profile,
          remark: values.remark || "",
          log_level: values.logLevel ?? "info",
          sources: [...sources, ...rawSources],
          custom_node_ids: values.selectedCustomNodeIds ?? [],
          use_system_rules: values.useSystemRuleList ?? true,
          use_system_groups: values.useSystemGroup ?? true,
          use_system_filters: values.useSystemFilter ?? true,
          use_system_custom_config: values.useSystemCustomConfig ?? true,
          use_system_dns: values.useSystemDnsConfig ?? true,
          editor: {
            rule_list: values.ruleList || "",
            group: values.group || "",
            filter: values.filter || "",
            custom_config: values.customConfig || "",
            dns_config: values.dnsConfig || "",
            private_access_config: values.privateAccessConfig || "",
            servers: values.servers || "[]",
          },
        });
      } catch (error) {
        console.error(error);
      }
    };

    const manualServerCount = (() => {
      try {
        const parsed = parseJsonc(manualServers || "[]");
        return Array.isArray(parsed) ? parsed.length : 0;
      } catch {
        return 0;
      }
    })();

    const openManualServersEditor = () => {
      setManualServersDraft(manualServers || JSON.stringify([], null, 2));
      setManualServersEditorOpen(true);
    };

    const saveManualServers = (): undefined => {
      if (isValidJsonc(manualServersDraft)) {
        form.setFieldValue("servers", manualServersDraft);
        setManualServersEditorOpen(false);
      } else {
        message.error(`servers ${t("proxy.form.jsonFormatError")}`);
      }
      return undefined;
    };

    // 配置字段定义（用于统一渲染 useSystem checkbox + editor）
    type ConfigField = "ruleList" | "group" | "customConfig";
    const CONFIG_FIELDS: {
      field: ConfigField;
      useSystemField: string;
      tab: string;
      labelKey: string;
      placeholderKey: string;
    }[] = [
      {
        field: "ruleList",
        useSystemField: "useSystemRuleList",
        tab: "ruleList",
        labelKey: "proxy.form.ruleListLabel",
        placeholderKey: "proxy.form.ruleListPlaceholder",
      },
      {
        field: "group",
        useSystemField: "useSystemGroup",
        tab: "group",
        labelKey: "proxy.form.groupLabel",
        placeholderKey: "proxy.form.groupPlaceholder",
      },
      {
        field: "customConfig",
        useSystemField: "useSystemCustomConfig",
        tab: "customConfig",
        labelKey: "proxy.form.customConfigLabel",
        placeholderKey: "proxy.form.customConfigPlaceholder",
      },
    ];

    return (
      <div className="min-h-0 rounded-lg border border-black/[0.08] bg-white/50 p-4 dark:border-white/[0.08] dark:bg-white/[0.02]">
          <div className="mb-4 shrink-0">
            <Tabs
              type="segment"
              activeKey={activeTab}
              onChange={(key) => setActiveTab(key)}
              items={localizedTabs.map((tab) => ({
                key: tab.value,
                label: tab.label,
              }))}
            />
          </div>

          <Form form={form} layout="vertical">
            {/* 基础信息 */}
            <div style={{ display: activeTab === "basic" ? "block" : "none" }}>
              <Form.Item label={t("proxy.form.remark")} name="remark">
                <TextArea
                  rows={3}
                  placeholder={t("proxy.form.remarkPlaceholder")}
                />
              </Form.Item>
              <Form.Item
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
              </Form.Item>
            </div>

            {/* 订阅源 */}
            <div
              style={{
                display: activeTab === "subscribeUrl" ? "block" : "none",
              }}
            >
              <Form.Item
                label={t("proxy.form.subscribeUrlLabel")}
                name="subscribeItems"
              >
                <SubscribeItemsEditor />
              </Form.Item>

              {rawSources.map((source, index) => (
                <div key={source.id} className="mt-3 rounded-lg border border-gray-200 p-3 dark:border-gray-700">
                  <div className="mb-2 flex items-center gap-2">
                    <Tag>RAW</Tag>
                    <input
                      className="min-w-0 flex-1 bg-transparent text-sm outline-none"
                      value={source.remark ?? ""}
                      placeholder={t("proxy.form.subscribeItemRemark")}
                      onChange={(event) => setRawSources((current) => current.map((item, position) => position === index ? { ...item, remark: event.target.value } : item))}
                    />
                    <Button variant="text" size="small" danger onClick={() => setRawSources((current) => current.filter((_, position) => position !== index))}>
                      {t("proxy.actions.delete")}
                    </Button>
                  </div>
                  <TextArea
                    rows={8}
                    value={source.content ?? ""}
                    placeholder="proxies:"
                    onChange={(event) => setRawSources((current) => current.map((item, position) => position === index ? { ...item, content: event.target.value } : item))}
                  />
                </div>
              ))}
              <Button
                className="!mt-3"
                variant="dashed"
                block
                onClick={() => setRawSources((current) => [...current, { id: crypto.randomUUID(), type: "raw", enabled: true, content: "", remark: "" }])}
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
                  defaultValue={defaults.filter || "[]"}
                />
              </div>
            </div>

            {/* 规则列表 / 分组 / 过滤器 / 自定义配置 */}
            {CONFIG_FIELDS.map(
              ({ field, useSystemField, tab, labelKey, placeholderKey }) => (
                <div key={tab} className={activeTab === tab ? "" : "hidden"}>
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
                      defaultValue={field === "ruleList" ? defaults.rule_list : field === "group" ? defaults.group : defaults.custom_config}
                      placeholder={t(placeholderKey)}
                    />
                  </Form.Item>
                </div>
              ),
            )}

            {/* DNS 配置 */}
            <div className={activeTab === "dnsConfig" ? "" : "hidden"}>
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
                  defaultValue={defaults.dns_config}
                />
              </Form.Item>
            </div>

            {/* 内网访问配置 */}
            <div
              className={activeTab === "privateAccessConfig" ? "" : "hidden"}
            >
              <Form.Item
                label={t("proxy.form.privateAccessConfigLabel")}
                name="privateAccessConfig"
              >
                <PrivateAccessEditor />
              </Form.Item>
            </div>

            {/* 额外服务器 */}
            <div className={activeTab === "servers" ? "" : "hidden"}>
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
          <div className="mt-5 flex justify-end gap-2 border-t border-gray-200 pt-4 dark:border-gray-700">
            <Button onClick={onCancel}>{t("common.cancel")}</Button>
            <Button variant="primary" loading={saving} onClick={handleSubmit}>{t("common.save")}</Button>
          </div>
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
        </Modal>
      </div>
    );
};

export default ProxySubscribeModal;
