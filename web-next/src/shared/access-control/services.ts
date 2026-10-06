import type { AccessControlBootstrapPayload } from "./contracts";

import { getSessionContext } from "@/shared/session";

export async function getAccessControlBootstrap(): Promise<AccessControlBootstrapPayload> {
  const sessionContext = await getSessionContext();

  return {
    capabilities: sessionContext.access_control?.capabilities ?? [],
    surface: sessionContext.access_control?.surface ?? "user",
  };
}
