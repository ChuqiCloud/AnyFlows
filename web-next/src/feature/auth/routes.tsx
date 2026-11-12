import { useEffect } from "react";
import { useLocation, useNavigate, useSearchParams } from "react-router-dom";

import { LoginPage, type LoginNotice } from "@/features/auth/login-page";
import { readInvitationContext } from "@/features/auth/invitation-context";
import { OAuthCallbackPage } from "@/features/auth/oauth-callback-page";
import { ResetPasswordPage } from "@/features/auth/reset-password-page";
import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";
import { sanitizeReturnPath } from "@/shared/auth/kernel";

import authModule from "./module";

// 公开入口不要求已登录会话，因此显式关闭 requireAuth。
const routeMeta = createModuleRouteMetaResolver(authModule, {
  requireAuth: false,
});

// 登录、重置密码与回调页需要透传导航回调，lazyRoute 无法传 props，故保留本地适配组件。
function LoginRoute() {
  const notice = (useLocation().state as { notice?: LoginNotice } | null)?.notice;
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const invitation = readInvitationContext(params);
  const returnPath = sanitizeReturnPath(params.get("from"));
  useEffect(() => {
    try {
      sessionStorage.setItem("anyflows.auth.login-return-path", returnPath);
    } catch {
      // Browser privacy settings can disable storage; password and passkey login still work.
    }
  }, [returnPath]);

  return (
    <LoginPage
      notice={notice}
      inviteCode={invitation.kind === "valid" ? invitation.code : undefined}
      invitationInvalid={invitation.kind === "invalid"}
      onAuthenticated={() => {
        try { sessionStorage.removeItem("anyflows.auth.login-return-path"); } catch { /* Storage is optional. */ }
        navigate(returnPath, { replace: true });
      }}
    />
  );
}

function ResetPasswordRoute() {
  const navigate = useNavigate();
  const [params] = useSearchParams();

  return (
    <ResetPasswordPage
      token={params.get("token") ?? undefined}
      onCompleted={() =>
        navigate("/login", { replace: true, state: { notice: "passwordResetSuccess" } })
      }
    />
  );
}

function OAuthCallbackRoute() {
  const navigate = useNavigate();

  return (
    <OAuthCallbackPage
      onAuthenticated={() => {
        let returnPath: string | null = null;
        try {
          returnPath = sessionStorage.getItem("anyflows.auth.login-return-path");
          sessionStorage.removeItem("anyflows.auth.login-return-path");
        } catch {
          // Fall back to the default console route.
        }
        navigate(sanitizeReturnPath(returnPath), { replace: true });
      }}
    />
  );
}

export const authRoutes: AppRouteObject[] = [
  {
    path: "/login",
    element: <LoginRoute />,
    meta: routeMeta("/login"),
  },
  {
    path: "/forgot-password",
    element: lazyRoute(
      () => import("@/features/auth/forgot-password-page"),
      "ForgotPasswordPage",
    ),
    meta: routeMeta("/forgot-password"),
  },
  {
    path: "/reset-password",
    element: <ResetPasswordRoute />,
    meta: routeMeta("/reset-password"),
  },
  {
    path: "/oauth/callback",
    element: <OAuthCallbackRoute />,
    meta: routeMeta("/oauth/callback"),
  },
];
