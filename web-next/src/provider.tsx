import { useHref, useNavigate } from "react-router-dom";
import { HeroUIProvider } from "@heroui/system";
import { Provider as ReduxProvider } from "react-redux";
import React from "react";

import { PlatformProvider } from "@/shared/platform-provider";
import { store } from "@/shared/store";

export function Provider({ children }: { children: React.ReactNode }) {
  const navigate = useNavigate();

  return (
    <ReduxProvider store={store}>
      <PlatformProvider>
        <HeroUIProvider navigate={navigate} useHref={useHref}>
          {children}
        </HeroUIProvider>
      </PlatformProvider>
    </ReduxProvider>
  );
}
