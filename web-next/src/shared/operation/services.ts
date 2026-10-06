import type {
  CreateOperationChallengeInput,
  CreateOperationChallengeResponse,
  OperationChallengePayload,
  OperationChallengeStatusResponse,
  PollOperationChallengeStatusInput,
} from "./contracts";
import type { OperationChallenge } from "./types";

import { request } from "@/shared/http";

export const OPERATION_GRANT_HEADER = "X-Operation-Grant";

const toQrCodeUrl = (qrCode?: string) => {
  if (!qrCode) {
    return undefined;
  }

  if (qrCode.startsWith("data:")) {
    return qrCode;
  }

  return `data:image/png;base64,${qrCode}`;
};

const toOperationChallenge = (
  payload: OperationChallengePayload,
): OperationChallenge => ({
  challengeId: payload.challenge_id,
  operationType: payload.operation_type,
  operationDigest: payload.operation_digest,
  title: payload.operation_title,
  summary: payload.operation_summary,
  status: payload.status,
  pollToken: payload.poll_token,
  qrCode: toQrCodeUrl(payload.qr_code),
  qrPayload: payload.qr_payload,
  grantToken: payload.grant_token,
  rejectReason: payload.reject_reason,
  expiresIn: payload.expires_in,
  expiresAt: payload.expires_at ?? null,
  approvedAt: payload.approved_at ?? null,
  rejectedAt: payload.rejected_at ?? null,
  consumedAt: payload.consumed_at ?? null,
  grantExpiresAt: payload.grant_expires_at ?? null,
});

export async function createOperationChallenge(
  input: CreateOperationChallengeInput,
): Promise<OperationChallenge> {
  const response = await request.post<CreateOperationChallengeResponse>(
    "/behavior/operation/challenges",
    {
      operation_type: input.operationType,
      operation_title: input.title,
      operation_summary: input.summary,
    },
  );

  return toOperationChallenge(response.data);
}

export async function getOperationChallengeStatus(
  input: PollOperationChallengeStatusInput,
): Promise<OperationChallenge> {
  const response = await request.post<OperationChallengeStatusResponse>(
    "/behavior/operation/challenges/status",
    {
      challenge_id: input.challengeId,
      poll_token: input.pollToken,
    },
  );

  return toOperationChallenge(response.data);
}

export const buildOperationGrantHeaders = (grantToken?: string | null) =>
  grantToken
    ? {
        [OPERATION_GRANT_HEADER]: grantToken,
      }
    : undefined;
