import type { ReactNode } from "react";

import {
  AccessControlProvider,
  AccessControlRuntimeProvider,
} from "@/shared/access-control";
import { forgeNavigationNodes } from "@/feature/modules";
import { AuthRuntimeProvider } from "@/shared/auth/runtime";
import { NavigationProvider } from "@/shared/navigation";
import { OperationProvider } from "@/shared/operation";
import {
  SessionContextRuntimeProvider,
  SessionProvider,
} from "@/shared/session";
import { WSRuntimeProvider } from "@/shared/ws";

export interface PlatformProviderProps {
  children: ReactNode;
}

export function PlatformProvider({ children }: PlatformProviderProps) {
  return (
    <SessionProvider>
      <AccessControlProvider>
        <OperationProvider>
          <NavigationProvider initialRegistry={forgeNavigationNodes}>
            <AuthRuntimeProvider>
              <WSRuntimeProvider>
                <SessionContextRuntimeProvider>
                  <AccessControlRuntimeProvider>{children}</AccessControlRuntimeProvider>
                </SessionContextRuntimeProvider>
              </WSRuntimeProvider>
            </AuthRuntimeProvider>
          </NavigationProvider>
        </OperationProvider>
      </AccessControlProvider>
    </SessionProvider>
  );
}
