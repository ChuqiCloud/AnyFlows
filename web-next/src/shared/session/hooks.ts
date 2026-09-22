import { useContext } from "react";

import { SessionContext } from "./context";

export function useSession() {
  const context = useContext(SessionContext);

  if (!context) {
    throw new Error("useSession 必须在 SessionProvider 内使用");
  }

  return context;
}
