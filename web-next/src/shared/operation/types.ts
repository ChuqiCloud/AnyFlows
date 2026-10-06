export type OperationChallengeStatus =
  | "pending"
  | "approved"
  | "rejected"
  | "expired"
  | "consumed";

export interface OperationChallenge {
  challengeId: string;
  operationType: string;
  operationDigest?: string;
  title?: string;
  summary?: string;
  status: OperationChallengeStatus;
  pollToken?: string;
  qrCode?: string;
  qrPayload?: string;
  grantToken?: string;
  rejectReason?: string;
  expiresIn?: number;
  expiresAt?: string | null;
  approvedAt?: string | null;
  rejectedAt?: string | null;
  consumedAt?: string | null;
  grantExpiresAt?: string | null;
}

export interface OperationGrant {
  token: string;
  operationType?: string;
  expiresAt?: string | null;
}

export interface OperationSnapshot {
  currentChallenge: OperationChallenge | null;
  currentGrant: OperationGrant | null;
  isReady: boolean;
}

export interface OperationContextValue extends OperationSnapshot {
  setSnapshot: (snapshot: Partial<OperationSnapshot>) => void;
  beginChallenge: (challenge: OperationChallenge) => void;
  resolveChallenge: (patch: Partial<OperationChallenge>) => void;
  setGrant: (grant: OperationGrant | null) => void;
  consumeGrant: () => string | null;
  clearChallenge: () => void;
  reset: () => void;
}
