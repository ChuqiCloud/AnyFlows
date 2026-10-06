import type { AccessControlContextValue } from "./types";

import { createContext } from "react";

export const AccessControlContext =
  createContext<AccessControlContextValue | null>(null);
