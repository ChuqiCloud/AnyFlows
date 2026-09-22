import type { ReactNode } from "react";
import type { AuthState } from "@/shared/auth/kernel";

import { AuthLoading } from "@/shared/components/loading";
import {
  isAuthAuthenticated,
  isAuthReady,
} from "@/shared/auth/kernel";
import { useSession } from "@/shared/session";

interface AuthGateViewProps {
  children: ReactNode;
  fallback?: ReactNode;
  authState: AuthState;
}

function AuthGateView({
  children,
  fallback,
  authState,
}: AuthGateViewProps) {
  if (!isAuthReady(authState)) {
    return <AuthLoading />;
  }

  if (!isAuthAuthenticated(authState)) {
    return null;
  }

  if (fallback) {
    return <>{fallback}</>;
  }

  return <>{children}</>;
}

export interface AuthGateProps {
  children: ReactNode;
}

export function AuthGate({ children }: AuthGateProps) {
  const session = useSession();

  return (
    <AuthGateView authState={session.authState}>
      {children}
    </AuthGateView>
  );
}

export { AuthGateView };
