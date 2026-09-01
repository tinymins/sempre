import { Form } from "@acme/components";
import Editor, { type Monaco } from "@monaco-editor/react";
import { parse as parseJsonc } from "jsonc-parser";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import "@/lib/monaco";
import type { LinuxNetworkInventory, SubscriptionConfigurationContext } from "@/lib/types";
import DnsConfigEditor from "./DnsConfigEditor";
import TagListEditor from "./TagListEditor";


// JSONC 编辑器组件，支持 // 和 /* */ 注释
interface JsoncEditorProps {
  value?: string;
  onChange?: (value: string) => void;
  placeholder?: string;
  readOnly?: boolean;
}

export const JsoncEditor = ({ value, onChange, readOnly }: JsoncEditorProps) => {
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

export const ConfigFieldEditor = ({
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
	configurationContext: SubscriptionConfigurationContext;
	networkInventory?: LinuxNetworkInventory;
}

export const DnsConfigEditorField = ({
  value,
  onChange,
  form,
	defaultValue,
	configurationContext,
	networkInventory,
}: DnsConfigEditorFieldProps) => {
  const useSystem = Form.useWatch("useSystemDnsConfig", form);
	const systemDnsListenHostOptions = useMemo(() => systemDnsListenOptions(networkInventory), [networkInventory]);
	const features = useMemo(() => {
		if (!['windows', 'macos'].includes(configurationContext.platform)) return configurationContext.capabilities.features;
		return configurationContext.capabilities.features.filter((feature) => feature !== 'dns.system_takeover');
	}, [configurationContext.capabilities.features, configurationContext.platform]);

  if (useSystem) {
    return (
		<DnsConfigEditor key="system-default" value={defaultValue} readOnly features={features} systemDnsListenHostOptions={systemDnsListenHostOptions} />
    );
  }

  return (
    <DnsConfigEditor
		key="user-custom"
		value={value}
		onChange={onChange}
		features={features}
		systemDnsListenHostOptions={systemDnsListenHostOptions}
	/>
  );
};

function systemDnsListenOptions(inventory?: LinuxNetworkInventory) {
	const options: Array<{ value: string; label: string }> = [];
	const seen = new Set(["127.0.0.1", "0.0.0.0"]);
	for (const item of inventory?.interfaces ?? []) {
		if (!item.up) continue;
		for (const value of item.addresses) {
			const host = value.split("/")[0]?.trim();
			if (!host || seen.has(host) || !isIPv4Address(host)) continue;
			seen.add(host);
			options.push({ value: host, label: `${host} · ${item.name}` });
		}
	}
	return options;
}

function isIPv4Address(value: string) {
	const parts = value.split(".");
	return parts.length === 4 && parts.every((part) => {
		if (!/^\d+$/.test(part)) return false;
		const number = Number(part);
		return number >= 0 && number <= 255 && String(number) === String(Number.parseInt(part, 10));
	});
}

/**
 * Node filter field: bridges JSON string ↔ string[] for TagListEditor.
 * Uses useSystemFilter to toggle between system default (read-only) and user custom.
 */
interface NodeFilterFieldProps {
  form: ReturnType<typeof Form.useForm>[0];
  defaultValue: string;
}

export const NodeFilterField = ({ form, defaultValue }: NodeFilterFieldProps) => {
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

export const NodeFilterTagAdapter = ({
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
