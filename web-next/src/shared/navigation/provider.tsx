import type { ReactNode } from "react";
import type { NavNode, NavigationContextValue } from "./types";

import { useCallback, useMemo, useState } from "react";

import { NavigationContext } from "./context";
import { mergeNavigationNodes } from "./registry";

export interface NavigationProviderProps {
  children: ReactNode;
  initialRegistry?: NavNode[];
}

export function NavigationProvider({
  children,
  initialRegistry = [],
}: NavigationProviderProps) {
  const [registry, setRegistry] = useState<NavNode[]>(initialRegistry);

  const register = useCallback((nodes: NavNode[]) => {
    setRegistry((current) => mergeNavigationNodes(current, nodes));
  }, []);

  const replace = useCallback((nodes: NavNode[]) => {
    setRegistry((current) => {
      const incomingKeys = new Set(nodes.map((node) => node.key));
      const preserved = current.filter((node) => !incomingKeys.has(node.key));

      return mergeNavigationNodes(preserved, nodes);
    });
  }, []);

  const reset = useCallback(() => {
    setRegistry([]);
  }, []);

  const value = useMemo<NavigationContextValue>(
    () => ({
      registry,
      register,
      replace,
      reset,
    }),
    [register, registry, replace, reset],
  );

  return (
    <NavigationContext.Provider value={value}>
      {children}
    </NavigationContext.Provider>
  );
}
