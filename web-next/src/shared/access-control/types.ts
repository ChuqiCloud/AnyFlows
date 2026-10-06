export type AccessSurface = "public" | "user" | "admin";
export type AccessResourceStatus = "idle" | "loading" | "ready" | "error";

export interface AccessSnapshot {
  capabilities: string[];
  surface: AccessSurface;
  status: AccessResourceStatus;
  error: string | null;
}

export interface AccessControlContextValue extends AccessSnapshot {
  isReady: boolean;
  hasCapability: (requirement?: string | string[]) => boolean;
  canAccessSurface: (surface?: AccessSurface) => boolean;
  setSnapshot: (snapshot: Partial<AccessSnapshot>) => void;
  reset: () => void;
}
