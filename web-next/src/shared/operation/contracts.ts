import type { OperationChallengeStatus } from "./types";

export interface OperationChallengePayload {
  challenge_id: string;
  status: OperationChallengeStatus;
  operation_type: string;
  operation_digest?: string;
  operation_title?: string;
  operation_summary?: string;
  expires_at?: string | null;
  expires_in?: number;
  poll_token?: string;
  qr_payload?: string;
  qr_code?: string;
  approved_at?: string | null;
  rejected_at?: string | null;
  consumed_at?: string | null;
  reject_reason?: string;
  grant_token?: string;
  grant_expires_at?: string | null;
}

export interface CreateOperationChallengeInput {
  operationType: string;
  title?: string;
  summary?: string;
}

export interface CreateOperationChallengeResponse
  extends OperationChallengePayload {}

export interface PollOperationChallengeStatusInput {
  challengeId: string;
  pollToken: string;
}

export interface OperationChallengeStatusResponse
  extends OperationChallengePayload {}
