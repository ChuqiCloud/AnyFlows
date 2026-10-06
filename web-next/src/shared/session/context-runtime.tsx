import type { ReactNode } from "react";

import { useEffect, useMemo, useRef } from "react";
import { useDispatch } from "react-redux";

import type { AppDispatch } from "@/shared/store";
import {
  isAuthAuthenticated,
  isAuthReady,
} from "@/shared/auth/kernel";

import { useSession } from "./hooks";
import { syncPlatformSessionContext } from "./runtime";

interface SessionContextRuntimeProviderProps {
  children: ReactNode;
}

export function SessionContextRuntimeProvider({
  children,
}: SessionContextRuntimeProviderProps) {
  const dispatch = useDispatch<AppDispatch>();
  const session = useSession();
  const lastSyncedKeyRef = useRef<string | null>(null);
  const inFlightKeyRef = useRef<string | null>(null);
  const authReady = isAuthReady(session.authState);
  const authAuthenticated = isAuthAuthenticated(session.authState);

  const syncKey = useMemo(
    () =>
      [
        authAuthenticated ? "1" : "0",
        session.principal?.principal_id ?? "",
      ].join(":"),
    [authAuthenticated, session.principal?.principal_id],
  );

  useEffect(() => {
    if (!authReady || !authAuthenticated) {
      lastSyncedKeyRef.current = null;
      inFlightKeyRef.current = null;
      return;
    }

    if (
      lastSyncedKeyRef.current === syncKey ||
      inFlightKeyRef.current === syncKey
    ) {
      return;
    }

    let cancelled = false;
    inFlightKeyRef.current = syncKey;

    const sync = async () => {
      try {
        const sessionContext = await syncPlatformSessionContext(dispatch);

        if (cancelled || !sessionContext) {
          return;
        }

        lastSyncedKeyRef.current = `1:${sessionContext.principal.principal_id}`;
      } finally {
        if (inFlightKeyRef.current === syncKey) {
          inFlightKeyRef.current = null;
        }
      }
    };

    void sync();

    return () => {
      cancelled = true;

      if (inFlightKeyRef.current === syncKey) {
        inFlightKeyRef.current = null;
      }
    };
  }, [authAuthenticated, authReady, dispatch, syncKey]);

  return <>{children}</>;
}
