import type { ReactNode } from "react";
import type {
  SessionBootstrapPayload,
  SessionContextValue,
  SessionState,
} from "./types";

import { useCallback, useEffect, useMemo, useRef } from "react";

import {
  bootstrapSession,
  clearSession,
  hydrateSessionState,
  setSessionContextState,
  selectSessionState,
} from "@/shared/store/session";
import { useAppDispatch, useAppSelector } from "@/shared/store/hooks";

import { SessionContext } from "./context";
import { syncPlatformSessionContext } from "./runtime";

export interface SessionProviderProps {
  children: ReactNode;
  initialValue?: Partial<SessionState>;
}

export function SessionProvider({
  children,
  initialValue,
}: SessionProviderProps) {
  const dispatch = useAppDispatch();
  const state = useAppSelector(selectSessionState);
  const hydratedRef = useRef(false);

  useEffect(() => {
    if (hydratedRef.current || !initialValue) {
      return;
    }

    dispatch(hydrateSessionState(initialValue));
    hydratedRef.current = true;
  }, [dispatch, initialValue]);

  const bootstrap = useCallback((payload: SessionBootstrapPayload) => {
    dispatch(bootstrapSession(payload));
  }, [dispatch]);

  const clear = useCallback(() => {
    dispatch(clearSession());
  }, [dispatch]);

  const setContextState = useCallback((payload: {
    status: SessionState["contextStatus"];
    error?: string | null;
  }) => {
    dispatch(setSessionContextState(payload));
  }, [dispatch]);

  const retryContextSync = useCallback(async () => {
    const sessionContext = await syncPlatformSessionContext(dispatch);

    return Boolean(sessionContext);
  }, [dispatch]);

  const value = useMemo<SessionContextValue>(
    () => ({
      ...state,
      bootstrap,
      clear,
      setContextState,
      retryContextSync,
    }),
    [bootstrap, clear, retryContextSync, setContextState, state],
  );

  return (
    <SessionContext.Provider value={value}>{children}</SessionContext.Provider>
  );
}
