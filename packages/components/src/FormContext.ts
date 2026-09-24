import { createContext, useContext } from "react";
import type { FormInstance } from "./Form.types";

export const FormContext = createContext<FormInstance | null>(null);

export function useFormContext() {
  return useContext(FormContext);
}
