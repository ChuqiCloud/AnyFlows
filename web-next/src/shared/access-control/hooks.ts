import { useContext } from "react";

import { AccessControlContext } from "./context";

export function useAccessControl() {
  const context = useContext(AccessControlContext);

  if (!context) {
    throw new Error("useAccessControl 必须在 AccessControlProvider 内使用");
  }

  return context;
}
