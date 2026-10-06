import type { AppDispatch } from "@/shared/store";
import type { SessionContext } from "./contracts";

import {
  clearStoredAuthSession,
  deriveAuthState,
  dispatchAuthStateChangedEvent,
  restoreAuthSession,
  tokenManager,
} from "@/shared/auth/kernel";
import { resetAccessControl } from "@/shared/store/access-control";
import { resetOperation } from "@/shared/store/operation";
import {
  bootstrapSession,
  clearSession,
  setSessionAuthState,
  setSessionContextState,
} from "@/shared/store/session";
import { clearUserMenu } from "@/shared/store/user-menu";
import { resolveCurrentTenantId, tenantStorage } from "@/shared/tenant";
import { setAccessSnapshot } from "@/shared/store/access-control";

import {
  clearSessionContextCache,
  getSessionContext,
} from "./services";

let inFlightSessionContextSync: Promise<SessionContext | null> | null = null;

export function applySessionContext(
  dispatch: AppDispatch,
  sessionContext: SessionContext,
) {
  const currentTenantId = resolveCurrentTenantId(
    sessionContext.tenants,
    sessionContext.default_tenant_id,
  );

  tokenManager.setPrincipal(sessionContext.principal);
  tenantStorage.setCurrentTenantId(currentTenantId);

  const authState = deriveAuthState({
    principal: sessionContext.principal,
    accessToken: tokenManager.getAccessToken(),
    isAuthenticated: true,
    isReady: true,
  });

  dispatch(
    bootstrapSession({
      principal: sessionContext.principal,
      realname: sessionContext.realname,
      authState,
    }),
  );
  dispatch(
    setAccessSnapshot({
      capabilities: sessionContext.access_control?.capabilities ?? [],
      surface: sessionContext.access_control?.surface ?? "user",
      status: "ready",
      error: null,
    }),
  );
  dispatchAuthStateChangedEvent(authState);
}

function applyRestoredAuthState(
  dispatch: AppDispatch,
  options?: {
    refreshed?: boolean;
    accessToken?: string | null;
  },
) {
  const principal = tokenManager.getPrincipal();
  const accessToken = options?.accessToken ?? tokenManager.getAccessToken();
  const authState =
    options?.refreshed
      ? {
          kind: "refreshing" as const,
          accessToken,
        }
      : deriveAuthState({
          principal,
          accessToken,
          isAuthenticated: true,
          isReady: true,
        });

  dispatch(
    bootstrapSession({
      principal,
      isAuthenticated: true,
      authState,
    }),
  );
  dispatchAuthStateChangedEvent(authState);
}

export function clearPlatformState(
  dispatch: AppDispatch,
  options?: {
    resetAccess?: boolean;
  },
) {
  clearStoredAuthSession();
  clearSessionContextCache();
  inFlightSessionContextSync = null;

  tenantStorage.clear();

  dispatch(clearSession());
  dispatchAuthStateChangedEvent({ kind: "anonymous" });
  dispatch(clearUserMenu());
  dispatch(resetOperation());

  if (options?.resetAccess ?? true) {
    dispatch(resetAccessControl());
  }
}

export async function bootstrapPlatformSession(dispatch: AppDispatch) {
  dispatch(setSessionAuthState({ kind: "restoring" }));
  dispatchAuthStateChangedEvent({ kind: "restoring" });

  try {
    const restored = await restoreAuthSession();

    if (!restored.ok) {
      clearPlatformState(dispatch, { resetAccess: true });

      return false;
    }

    applyRestoredAuthState(dispatch, {
      refreshed: restored.refreshed,
      accessToken: restored.accessToken,
    });

    return true;
  } catch {
    clearPlatformState(dispatch, { resetAccess: true });

    return false;
  }
}

export async function syncPlatformSessionContext(
  dispatch: AppDispatch,
): Promise<SessionContext | null> {
  if (inFlightSessionContextSync) {
    return inFlightSessionContextSync;
  }

  dispatch(
    setSessionContextState({
      status: "loading",
      error: null,
    }),
  );

  const requestPromise = (async () => {
    try {
      const sessionContext = await getSessionContext({ forceRefresh: true });

      applySessionContext(dispatch, sessionContext);
      dispatch(
        setSessionContextState({
          status: "ready",
          error: null,
        }),
      );

      return sessionContext;
    } catch {
      dispatch(
        setSessionContextState({
          status: "error",
          error: "获取会话上下文失败",
        }),
      );

      return null;
    }
  })().finally(() => {
    if (inFlightSessionContextSync === requestPromise) {
      inFlightSessionContextSync = null;
    }
  });

  inFlightSessionContextSync = requestPromise;

  return requestPromise;
}
