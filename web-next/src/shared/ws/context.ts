import { createContext } from "react";

import type { WSRuntimeContextValue } from "./types";

export const WSRuntimeContext = createContext<WSRuntimeContextValue | null>(null);
