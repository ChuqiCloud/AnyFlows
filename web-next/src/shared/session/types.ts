import type { AuthState } from "@/shared/auth/kernel";
import type {
  PrincipalProfile,
  RealnameBootstrap,
} from "./contracts";

export type SessionContextStatus =
  | "idle"
  | "loading"
  | "ready"
  | "error";

export interface SessionState {
  authState: AuthState;
  principal: PrincipalProfile | null;
  realname: RealnameBootstrap | null;
  isAuthenticated: boolean;
  isReady: boolean;
  contextStatus: SessionContextStatus;
  contextError: string | null;
}

export interface SessionBootstrapPayload {
  principal: PrincipalProfile | null;
  realname?: RealnameBootstrap | null;
  isAuthenticated?: boolean;
  authState?: AuthState;
}

export interface SessionContextValue extends SessionState {
  bootstrap: (payload: SessionBootstrapPayload) => void;
  clear: () => void;
  setContextState: (payload: {
    status: SessionContextStatus;
    error?: string | null;
  }) => void;
  retryContextSync: () => Promise<boolean>;
}
