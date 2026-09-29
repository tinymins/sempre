import { CodePanel, Modal, TextArea } from "@acme/components";
import { useState } from "react";
import { useEditorI18n as useTranslation } from "./i18n";
import type { PrivateConnectorForm } from "./private-model";
import {
  parseWireGuardImport,
  WireGuardImportError,
} from "./WireGuardImport";

interface Props {
  open: boolean;
  onCancel: () => void;
  onImport: (patch: Partial<PrivateConnectorForm>) => void;
}

export function WireGuardImportModal({ open, onCancel, onImport }: Props) {
  const { t } = useTranslation();
  const [value, setValue] = useState("");
  const [error, setError] = useState("");

  const confirm = () => {
    try {
      onImport(parseWireGuardImport(value));
      setValue("");
      setError("");
    } catch (reason) {
      const code = reason instanceof WireGuardImportError ? reason.code : "missingRequiredFields";
      setError(t(`proxy.form.privateWgImportError.${code}`));
    }
    return undefined;
  };

  const cancel = () => {
    setValue("");
    setError("");
    onCancel();
  };

  return (
    <Modal
      open={open}
      title={t("proxy.form.privateWgImportTitle")}
      okText={t("common.confirm")}
      cancelText={t("common.cancel")}
      okButtonProps={{ disabled: !value.trim() }}
      onOk={confirm}
      onCancel={cancel}
      destroyOnClose
    >
      <CodePanel padded={false}><TextArea
        rows={12}
        value={value}
        aria-label={t("proxy.form.privateWgImportInput")}
        placeholder={t("proxy.form.privateWgImportPlaceholder")}
        onChange={(event) => {
          setValue(event.target.value);
          setError("");
        }}
        className="!rounded-none !border-0 font-mono"
      /></CodePanel>
      {error ? <p role="alert" className="mt-2 text-sm text-red-500">{error}</p> : null}
    </Modal>
  );
}
