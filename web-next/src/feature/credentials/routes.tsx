import { useSearchParams } from "react-router-dom";

import { CredentialPage } from "@/features/credentials/credential-page";
import { createModuleRouteMetaResolver } from "@/shared/modules";
import type { AppRouteObject } from "@/shared/router";

import credentialsModule from "./module";

const routeMeta = createModuleRouteMetaResolver(credentialsModule, {
  requireAuth: true,
});

/** 只接受单个十进制正整数查询参数，避免宽松转换产生错误深链。 */
function positiveIntegerParam(params: URLSearchParams, key: string) {
  const values = params.getAll(key);

  if (values.length !== 1 || !/^[1-9]\d*$/.test(values[0])) {
    return undefined;
  }

  const value = Number(values[0]);

  return Number.isSafeInteger(value) ? value : undefined;
}

// 凭据页需要从查询参数接住渠道筛选，lazyRoute 无法透传 props，故在此适配后交给页面。
function CredentialsRoute() {
  const [params] = useSearchParams();

  return <CredentialPage initialChannelId={positiveIntegerParam(params, "channel")} />;
}

export const credentialsRoutes: AppRouteObject[] = [
  {
    path: "/console/credentials",
    element: <CredentialsRoute />,
    meta: routeMeta("/console/credentials"),
  },
];
