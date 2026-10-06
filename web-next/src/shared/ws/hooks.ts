import { useContext } from "react";

import { WSRuntimeContext } from "./context";

export function useWSRuntime() {
  const context = useContext(WSRuntimeContext);

  if (!context) {
    throw new Error("useWSRuntime 必须在 WSRuntimeProvider 内使用");
  }

  return context;
}
