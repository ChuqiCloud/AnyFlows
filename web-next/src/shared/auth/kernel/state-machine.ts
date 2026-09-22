import type { PrincipalProfile } from "@/shared/session/contracts";

export type AuthState =
  | { kind: "anonymous" }
  | { kind: "restoring" }
  | { kind: "exchanging" }
  | { kind: "authenticated"; accessToken: string | null }
  | { kind: "refreshing"; accessToken: string | null }
  | { kind: "error"; code: string; message: string };

export function deriveAuthState(input: {
  principal: PrincipalProfile | null;
  accessToken?: string | null;
  isAuthenticated?: boolean;
  isReady?: boolean;
  previous?: AuthState;
}): AuthState {
  const {
    accessToken = null,
    isAuthenticated = false,
    isReady = false,
    previous,
  } = input;

  if (!isReady) {
    return { kind: "restoring" };
  }

  if (isAuthenticated) {
    if (previous?.kind === "refreshing") {
      return { kind: "refreshing", accessToken };
    }

    return { kind: "authenticated", accessToken };
  }

  if (previous?.kind === "error") {
    return previous;
  }

  return { kind: "anonymous" };
}

export function isAuthReady(authState: AuthState): boolean {
  return authState.kind !== "restoring" && authState.kind !== "exchanging";
}

export function isAuthAuthenticated(authState: AuthState): boolean {
  return (
    authState.kind === "authenticated" || authState.kind === "refreshing"
  );
}
