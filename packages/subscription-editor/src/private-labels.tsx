import { Tooltip, WarningOutlined } from "@acme/components";
import type { ConnectorType } from "./private-model";
export const FieldLabel = ({ children }: { children: string }) => (
  <span className="text-xs text-gray-500 dark:text-gray-400">{children}</span>
);

export const connectorTypeLabel = (
  type: ConnectorType,
  wireguardWarning: string,
) => {
  if (type !== "wireguard") return type;
  return (
    <span className="inline-flex min-w-0 items-center gap-1">
      <span>WireGuard</span>
      <Tooltip title={wireguardWarning}>
        <span className="inline-flex text-amber-500">
          <WarningOutlined />
        </span>
      </Tooltip>
    </span>
  );
};
