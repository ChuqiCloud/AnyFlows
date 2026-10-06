import type { ReactNode } from "react";
import type {
  OperationChallenge,
  OperationContextValue,
  OperationGrant,
  OperationSnapshot,
} from "./types";

import { useCallback, useEffect, useMemo, useRef } from "react";

import {
  clearOperationChallenge,
  clearOperationGrant,
  hydrateOperationSnapshot,
  patchOperationChallenge,
  resetOperation,
  selectOperationState,
  setOperationChallenge,
  setOperationGrant,
  setOperationSnapshot,
} from "@/shared/store/operation";
import { useAppDispatch, useAppSelector } from "@/shared/store/hooks";

import { OperationContext } from "./context";

export interface OperationProviderProps {
  children: ReactNode;
  initialValue?: Partial<OperationSnapshot>;
}

export function OperationProvider({
  children,
  initialValue,
}: OperationProviderProps) {
  const dispatch = useAppDispatch();
  const snapshot = useAppSelector(selectOperationState);
  const hydratedRef = useRef(false);

  useEffect(() => {
    if (hydratedRef.current || !initialValue) {
      return;
    }

    dispatch(hydrateOperationSnapshot(initialValue));
    hydratedRef.current = true;
  }, [dispatch, initialValue]);

  const setSnapshot = useCallback(
    (next: Partial<OperationSnapshot>) => {
      dispatch(setOperationSnapshot(next));
    },
    [dispatch],
  );

  const beginChallenge = useCallback(
    (challenge: OperationChallenge) => {
      dispatch(setOperationChallenge(challenge));
    },
    [dispatch],
  );

  const resolveChallenge = useCallback(
    (patch: Partial<OperationChallenge>) => {
      dispatch(patchOperationChallenge(patch));
    },
    [dispatch],
  );

  const setGrant = useCallback(
    (grant: OperationGrant | null) => {
      dispatch(setOperationGrant(grant));
    },
    [dispatch],
  );

  const consumeGrant = useCallback(() => {
    const token = snapshot.currentGrant?.token ?? null;

    dispatch(clearOperationGrant());

    return token;
  }, [dispatch, snapshot.currentGrant?.token]);

  const clearChallenge = useCallback(() => {
    dispatch(clearOperationChallenge());
  }, [dispatch]);

  const reset = useCallback(() => {
    dispatch(resetOperation());
  }, [dispatch]);

  const value = useMemo<OperationContextValue>(
    () => ({
      ...snapshot,
      setSnapshot,
      beginChallenge,
      resolveChallenge,
      setGrant,
      consumeGrant,
      clearChallenge,
      reset,
    }),
    [
      beginChallenge,
      clearChallenge,
      consumeGrant,
      reset,
      resolveChallenge,
      setGrant,
      setSnapshot,
      snapshot,
    ],
  );

  return (
    <OperationContext.Provider value={value}>
      {children}
    </OperationContext.Provider>
  );
}
