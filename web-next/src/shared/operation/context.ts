import { createContext } from "react";

import type { OperationContextValue } from "./types";

export const OperationContext =
  createContext<OperationContextValue | null>(null);
