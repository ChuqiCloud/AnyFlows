import { useLocation, useNavigate, useSearchParams } from "react-router-dom";

import { LoginPage, type LoginNotice } from "@/features/auth/login-page";
import { OAuthCallbackPage } from "@/features/auth/oauth-callback-page";
import { ResetPasswordPage } from "@/features/auth/reset-password-page";
import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";

import authModule from "./module";

// 公开入口不要求已登录会话，因此显式关闭 requireAuth。
const routeMeta = createModuleRouteMetaResolver(authModule, {
  requireAuth: false,
});

// 登录、重置密码与回调页需要透传导航回调，lazyRoute 无法传 props，故保留本地适配组件。
function LoginRoute() {
  const notice = (useLocation().state as { notice?: LoginNotice } | null)?.notice;
  const navigate = useNavigate();

  return (
    <LoginPage
      notice={notice}
      onAuthenticated={() => navigate("/console", { replace: true })}
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
      onAuthenticated={() => navigate("/console", { replace: true })}
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
