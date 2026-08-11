import { type ReactNode, useCallback, useRef, useState } from "react";
import { Modal } from "./Modal";
import type { ConfirmConfig } from "./Modal.types";

/**
 * Hook-based confirm dialog that inherits ModalContainerContext.
 * Unlike Modal.confirm(), the dialog renders inside the current React tree
 * so it appears within FloatingWindow when used in a window context.
 *
 * Usage:
 *   const [confirmHolder, confirm] = useConfirm();
 *   confirm({ title: "Delete?", onOk: () => ... });
 *   return <>{confirmHolder}<div>...</div></>;
 */
export function useConfirm(): [ReactNode, (config: ConfirmConfig) => void] {
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const configRef = useRef<ConfirmConfig | null>(null);
  const loadingRef = useRef(false);
  const [, forceUpdate] = useState(0);

  const confirm = useCallback((config: ConfirmConfig) => {
    configRef.current = config;
    loadingRef.current = false;
    setLoading(false);
    setOpen(true);
    forceUpdate((n) => n + 1);
  }, []);

  const contextHolder = (
    <Modal
      open={open}
      title={configRef.current?.title}
      okText={configRef.current?.okText ?? "确定"}
      cancelText={configRef.current?.cancelText ?? "取消"}
      okButtonProps={configRef.current?.okButtonProps}
      confirmLoading={loading}
      maskClosable={!loading}
      onOk={async () => {
        loadingRef.current = true;
        setLoading(true);
        try {
          await configRef.current?.onOk?.();
          setOpen(false);
        } finally {
          loadingRef.current = false;
          setLoading(false);
        }
      }}
      onCancel={() => {
        if (loadingRef.current) return;
        configRef.current?.onCancel?.();
        setOpen(false);
      }}
    >
      {configRef.current?.content}
    </Modal>
  );

  return [contextHolder, confirm];
}
