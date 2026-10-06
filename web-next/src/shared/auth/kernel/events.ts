import type { WSEvent } from "@/shared/utils/ws-client";
import type { AuthState } from "./state-machine";

export type AuthInvalidationReason =
  | "unauthorized"
  | "session.logout"
  | "session.revoked"
  | "session.expired";

export interface AuthInvalidationEventDetail {
  reason: AuthInvalidationReason;
  source: "http" | "ws";
  event?: WSEvent;
}

export const AUTH_INVALIDATED_EVENT = "auth:invalidated";
export const AUTH_STATE_CHANGED_EVENT = "auth:state-changed";

export function dispatchAuthInvalidatedEvent(
  detail: AuthInvalidationEventDetail,
) {
  window.dispatchEvent(
    new CustomEvent<AuthInvalidationEventDetail>(AUTH_INVALIDATED_EVENT, {
      detail,
    }),
  );
}

export function dispatchAuthStateChangedEvent(state: AuthState) {
  window.dispatchEvent(
    new CustomEvent<AuthState>(AUTH_STATE_CHANGED_EVENT, {
      detail: state,
    }),
  );
}
