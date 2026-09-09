import { Modal, TextArea } from "@acme/components";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { PrivateConnectorForm } from "./PrivateAccessConfig";
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
      okText={t("proxy.common.confirm")}
      cancelText={t("proxy.common.cancel")}
      okButtonProps={{ disabled: !value.trim() }}
      onOk={confirm}
      onCancel={cancel}
      destroyOnClose
    >
      <TextArea
        rows={12}
        value={value}
        aria-label={t("proxy.form.privateWgImportInput")}
        placeholder={t("proxy.form.privateWgImportPlaceholder")}
        onChange={(event) => {
          setValue(event.target.value);
          setError("");
        }}
        className="font-mono"
      />
      {error ? <p role="alert" className="mt-2 text-sm text-red-500">{error}</p> : null}
    </Modal>
  );
}
