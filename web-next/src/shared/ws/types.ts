import type { WSEvent, WSEventType, WSConnectionState } from "@/shared/utils/ws-client";

export type WSRuntimeStatus =
  | "disconnected"
  | "connecting"
  | "connected"
  | "error";

export interface WSRuntimeState {
  status: WSRuntimeStatus;
  lastError: string | null;
  lastEvent: WSEvent | null;
}

export interface WSRuntimeContextValue extends WSRuntimeState {
  connect: () => Promise<void>;
  disconnect: () => void;
  on: (eventType: WSEventType | "*", listener: (event: WSEvent) => void) => () => void;
}

export const mapWSClientState = (state: WSConnectionState): WSRuntimeStatus =>
  state;
