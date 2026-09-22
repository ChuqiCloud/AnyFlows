import { useContext } from "react";

import { OperationContext } from "./context";

export function useOperation() {
  const context = useContext(OperationContext);

  if (!context) {
    throw new Error("useOperation 必须在 OperationProvider 内使用");
  }

  return context;
}
