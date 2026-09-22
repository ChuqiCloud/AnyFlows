import type { ReactNode } from "react";
import type {
  AccessControlContextValue,
  AccessSnapshot,
  AccessSurface,
} from "./types";

import { useCallback, useMemo } from "react";

import {
  resetAccessControl,
  selectAccessControlState,
  setAccessSnapshot,
} from "@/shared/store/access-control";
import { useAppDispatch, useAppSelector } from "@/shared/store/hooks";

import { AccessControlContext } from "./context";

const includesAll = (source: string[], requirement?: string | string[]) => {
  if (!requirement) {
    return true;
  }

  const required = Array.isArray(requirement) ? requirement : [requirement];

  return required.every((item) => source.includes(item));
};

export interface AccessControlProviderProps {
  children: ReactNode;
}

export function AccessControlProvider({
  children,
}: AccessControlProviderProps) {
  const dispatch = useAppDispatch();
  const snapshot = useAppSelector(selectAccessControlState);

  const setSnapshot = useCallback(
    (next: Partial<AccessSnapshot>) => {
      dispatch(setAccessSnapshot(next));
    },
    [dispatch],
  );

  const reset = useCallback(() => {
    dispatch(resetAccessControl());
  }, [dispatch]);

  const hasCapability = useCallback(
    (requirement?: string | string[]) =>
      includesAll(snapshot.capabilities, requirement),
    [snapshot.capabilities],
  );

  const canAccessSurface = useCallback(
    (surface?: AccessSurface) => {
      if (!surface || surface === "public") {
        return true;
      }

      if (surface === "user") {
        return snapshot.surface === "user" || snapshot.surface === "admin";
      }

      return snapshot.surface === surface;
    },
    [snapshot.surface],
  );

  const value = useMemo<AccessControlContextValue>(
    () => ({
      ...snapshot,
      isReady: snapshot.status === "ready",
      hasCapability,
      canAccessSurface,
      setSnapshot,
      reset,
    }),
    [canAccessSurface, hasCapability, reset, setSnapshot, snapshot],
  );

  return (
    <AccessControlContext.Provider value={value}>
      {children}
    </AccessControlContext.Provider>
  );
}
