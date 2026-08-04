import { FloatingPortal as BaseFloatingPortal } from "@floating-ui/react";
import { type ReactNode, useContext } from "react";
import { ModalContainerContext } from "./Modal";

export function FloatingPortal({ children }: { children: ReactNode }) {
  const container = useContext(ModalContainerContext);
  return (
    <BaseFloatingPortal root={container ?? undefined}>
      {children}
    </BaseFloatingPortal>
  );
}
