import type { ReactNode } from "react";
import type { WSEvent } from "@/shared/utils/ws-client";

import { useCallback, useEffect, useMemo, useState } from "react";

import {
  dispatchAuthInvalidatedEvent,
  isAuthAuthenticated,
  isAuthReady,
} from "@/shared/auth/kernel";
import { useSession } from "@/shared/session";
import { wsClient } from "@/shared/utils/ws-client";

import { WSRuntimeContext } from "./context";
import { mapWSClientState, type WSRuntimeContextValue } from "./types";

interface WSRuntimeProviderProps {
  children: ReactNode;
}

export function WSRuntimeProvider({ children }: WSRuntimeProviderProps) {
  const session = useSession();
  const [status, setStatus] = useState(mapWSClientState(wsClient.getState()));
  const [lastError, setLastError] = useState<string | null>(null);
  const [lastEvent, setLastEvent] = useState<WSEvent | null>(null);
  const authReady = isAuthReady(session.authState);
  const authAuthenticated = isAuthAuthenticated(session.authState);

  useEffect(() => {
    const unsubscribeState = wsClient.onStateChange((nextState) => {
      const mappedState = mapWSClientState(nextState);

      setStatus(mappedState);

      if (mappedState === "error") {
        setLastError("实时连接不可用");
      } else if (mappedState === "connected") {
        setLastError(null);
      }
    });

    const unsubscribeAll = wsClient.on("*", (event) => {
      setLastEvent(event);

      switch (event.type) {
        case "session.logout":
        case "session.revoked":
        case "session.expired":
          dispatchAuthInvalidatedEvent({
            reason: event.type,
            source: "ws",
            event,
          });
          break;
        default:
          break;
      }
    });

    return () => {
      unsubscribeState();
      unsubscribeAll();
    };
  }, []);

  useEffect(() => {
    if (!authReady || !authAuthenticated) {
      wsClient.disconnect();
      setStatus("disconnected");

      return;
    }

    void wsClient.connect();
  }, [authAuthenticated, authReady]);

  const connect = useCallback(async () => {
    await wsClient.connect();
  }, []);

  const disconnect = useCallback(() => {
    wsClient.disconnect();
    setStatus("disconnected");
  }, []);

  const on = useCallback<WSRuntimeContextValue["on"]>((eventType, listener) => {
    return wsClient.on(eventType, listener);
  }, []);

  const value = useMemo<WSRuntimeContextValue>(
    () => ({
      status,
      lastError,
      lastEvent,
      connect,
      disconnect,
      on,
    }),
    [connect, disconnect, lastError, lastEvent, on, status],
  );

  return (
    <WSRuntimeContext.Provider value={value}>{children}</WSRuntimeContext.Provider>
  );
}
