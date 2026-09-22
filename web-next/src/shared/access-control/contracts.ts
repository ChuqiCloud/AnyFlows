import type { AccessSurface } from "./types";

export interface AccessControlBootstrapPayload {
  capabilities: string[];
  surface: AccessSurface;
}
