import type { RouteGuardProps } from "./types";

import { AuthLoading } from "@/shared/components/loading";
import { useAccessControl } from "@/shared/access-control";
import {
  isAuthAuthenticated,
  isAuthReady,
} from "@/shared/auth/kernel";
import { useSession } from "@/shared/session";

export function RouteGuard({
  children,
  fallback = null,
  meta,
}: RouteGuardProps) {
  const session = useSession();
  const access = useAccessControl();

  if (!meta) {
    return <>{children}</>;
  }

  if (meta.requireAuth && !isAuthReady(session.authState)) {
    return <AuthLoading />;
  }

  if (meta.requireAuth && !isAuthAuthenticated(session.authState)) {
    return <>{fallback}</>;
  }

  if (
    access.status === "ready" &&
    meta.surface &&
    !access.canAccessSurface(meta.surface)
  ) {
    return <>{fallback}</>;
  }

  if (
    access.status === "ready" &&
    meta.capability &&
    !access.hasCapability(meta.capability)
  ) {
    return <>{fallback}</>;
  }

  return <>{children}</>;
}
