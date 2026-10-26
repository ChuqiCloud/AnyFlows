import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";
import { useSearchParams } from "react-router-dom";

import { ChannelModelPage } from "@/features/channels/channel-model-page";

import channelsModule from "./module";

const routeMeta = createModuleRouteMetaResolver(channelsModule, {
  requireAuth: true,
});

export const channelsRoutes: AppRouteObject[] = [
  {
    path: "/console/channels",
    element: lazyRoute(
      () => import("@/features/channels/channel-page"),
      "ChannelPage",
    ),
    meta: routeMeta("/console/channels"),
  },
  {
    path: "/console/channel-models",
    element: <ChannelModelsRoute />,
    meta: routeMeta("/console/channel-models"),
  },
];

function positiveIntegerParam(params: URLSearchParams, key: string) {
  const values = params.getAll(key);
  if (values.length !== 1 || !/^[1-9]\d*$/.test(values[0])) return undefined;
  const value = Number(values[0]);
  return Number.isSafeInteger(value) ? value : undefined;
}

function ChannelModelsRoute() {
  const [params] = useSearchParams();
  return <ChannelModelPage channelId={positiveIntegerParam(params, "channel")} />;
}
