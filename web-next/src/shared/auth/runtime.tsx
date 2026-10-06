import type { ReactNode } from "react";
import type { AuthState } from "@/shared/auth/kernel";

import { useEffect } from "react";
import { useDispatch } from "react-redux";
import { useLocation } from "react-router-dom";

import type { AppDispatch } from "@/shared/store";
import {
  AUTH_INVALIDATED_EVENT,
  AUTH_STATE_CHANGED_EVENT,
  getCurrentReturnPath,
  isAuthAuthenticated,
  isAuthReady,
  isProtectedPath,
  redirectToAuth,
} from "@/shared/auth/kernel";
import {
  bootstrapPlatformSession,
  clearPlatformState,
  useSession,
} from "@/shared/session";
import { setSessionAuthState } from "@/shared/store/session";

interface AuthRuntimeProviderProps {
  children: ReactNode;
}

/**
 * 统一处理恢复会话与登录跳转。
 * 公开页（模型广场、分享页等）不参与登录判定，只有受保护路径才引导到登录页。
 */
export function AuthRuntimeProvider({
  children,
}: AuthRuntimeProviderProps) {
  const dispatch = useDispatch<AppDispatch>();
  const location = useLocation();
  const session = useSession();
  const authReady = isAuthReady(session.authState);
  const authAuthenticated = isAuthAuthenticated(session.authState);
  const protectedPath = isProtectedPath(location.pathname);

  useEffect(() => {
    if (!protectedPath) {
      return;
    }

    const restore = async () => {
      if (session.authState.kind !== "restoring") {
        return;
      }

      const isAuthenticated = await bootstrapPlatformSession(dispatch);

      if (isAuthenticated) {
        return;
      }

      clearPlatformState(dispatch, { resetAccess: true });
    };

    void restore();
  }, [dispatch, protectedPath, session.authState.kind]);

  useEffect(() => {
    if (!protectedPath) {
      return;
    }

    const redirectToLogin = () => {
      clearPlatformState(dispatch, { resetAccess: true });
      void redirectToAuth(getCurrentReturnPath());
    };

    window.addEventListener(AUTH_INVALIDATED_EVENT, redirectToLogin);

    return () => {
      window.removeEventListener(AUTH_INVALIDATED_EVENT, redirectToLogin);
    };
  }, [dispatch, protectedPath]);

  useEffect(() => {
    const syncAuthState = (event: Event) => {
      const nextState = (event as CustomEvent<AuthState>).detail;

      if (!nextState) {
        return;
      }

      dispatch(setSessionAuthState(nextState));
    };

    window.addEventListener(AUTH_STATE_CHANGED_EVENT, syncAuthState);

    return () => {
      window.removeEventListener(AUTH_STATE_CHANGED_EVENT, syncAuthState);
    };
  }, [dispatch]);

  useEffect(() => {
    if (!protectedPath || !authReady || authAuthenticated) {
      return;
    }

    void redirectToAuth(getCurrentReturnPath());
  }, [authAuthenticated, authReady, protectedPath]);

  return <>{children}</>;
}
