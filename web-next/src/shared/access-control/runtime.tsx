import type { ReactNode } from "react";

import { useEffect } from "react";

import {
  isAuthAuthenticated,
  isAuthReady,
} from "@/shared/auth/kernel";
import { useSession } from "@/shared/session";

import { useAccessControl } from "./hooks";

interface AccessControlRuntimeProviderProps {
  children: ReactNode;
}

export function AccessControlRuntimeProvider({
  children,
}: AccessControlRuntimeProviderProps) {
  const session = useSession();
  const { reset } = useAccessControl();
  const authReady = isAuthReady(session.authState);
  const authAuthenticated = isAuthAuthenticated(session.authState);

  useEffect(() => {
    if (!authReady) {
      return;
    }

    if (!authAuthenticated) {
      reset();
    }
  }, [authAuthenticated, authReady, reset]);

  return <>{children}</>;
}
