import type {
  VerificationRequest,
  VerificationResponse,
} from "@/shared/components/two-factor-auth/types";

import { useCallback } from "react";

import {
  createOperationChallenge,
  getOperationChallengeStatus,
} from "./services";
import { useOperation } from "./hooks";

export interface OperationVerificationOptions {
  operationType: string;
  title: string;
  summary?: string;
  unsupportedMethodMessage?: string;
  startMessage?: string;
  pendingMessage?: string;
  rejectedMessage?: string;
  expiredMessage?: string;
  consumedMessage?: string;
  missingGrantMessage?: string;
}

const DEFAULT_UNSUPPORTED_METHOD_MESSAGE =
  "当前操作仅支持微信验证";
const DEFAULT_START_MESSAGE = "请扫描二维码继续验证";
const DEFAULT_PENDING_MESSAGE = "正在等待扫码确认";
const DEFAULT_SUCCESS_MESSAGE = "验证通过";
const DEFAULT_REJECTED_MESSAGE = "验证已被拒绝";
const DEFAULT_EXPIRED_MESSAGE = "验证已过期，请重试";
const DEFAULT_CONSUMED_MESSAGE = "授权已被使用，请重试";
const DEFAULT_MISSING_GRANT_MESSAGE =
  "验证已通过，但未返回授权令牌";

export function useOperationVerification(
  options: OperationVerificationOptions,
) {
  const {
    operationType,
    title,
    summary,
    unsupportedMethodMessage,
    startMessage,
    pendingMessage,
    rejectedMessage,
    expiredMessage,
    consumedMessage,
    missingGrantMessage,
  } = options;
  const {
    currentChallenge,
    beginChallenge,
    resolveChallenge,
    setGrant,
    clearChallenge,
    consumeGrant,
  } = useOperation();

  const reset = useCallback(() => {
    clearChallenge();
    setGrant(null);
  }, [clearChallenge, setGrant]);

  const verify = useCallback(
    async (
      verificationRequest: VerificationRequest,
    ): Promise<VerificationResponse> => {
      if (verificationRequest.method !== "wechat") {
        return {
          success: false,
          status: "error",
          message:
            unsupportedMethodMessage ?? DEFAULT_UNSUPPORTED_METHOD_MESSAGE,
        };
      }

      const activeChallenge =
        currentChallenge?.operationType === operationType
          ? currentChallenge
          : null;

      if (activeChallenge?.status === "pending" && activeChallenge.pollToken) {
        const nextChallenge = await getOperationChallengeStatus({
          challengeId: activeChallenge.challengeId,
          pollToken: activeChallenge.pollToken,
        });

        resolveChallenge(nextChallenge);

        if (nextChallenge.grantToken) {
          setGrant({
            token: nextChallenge.grantToken,
            operationType: nextChallenge.operationType,
            expiresAt:
              nextChallenge.grantExpiresAt ?? nextChallenge.expiresAt ?? null,
          });
        }

        switch (nextChallenge.status) {
          case "approved":
            if (!nextChallenge.grantToken) {
              reset();

              return {
                success: false,
                status: "error",
                message: missingGrantMessage ?? DEFAULT_MISSING_GRANT_MESSAGE,
              };
            }

            return {
              success: true,
              status: "success",
              message: DEFAULT_SUCCESS_MESSAGE,
              token: nextChallenge.grantToken,
            };
          case "rejected":
            reset();

            return {
              success: false,
              status: "error",
              message:
                nextChallenge.rejectReason ??
                rejectedMessage ??
                DEFAULT_REJECTED_MESSAGE,
            };
          case "expired":
            reset();

            return {
              success: false,
              status: "error",
              message: expiredMessage ?? DEFAULT_EXPIRED_MESSAGE,
            };
          case "consumed":
            reset();

            return {
              success: false,
              status: "error",
              message: consumedMessage ?? DEFAULT_CONSUMED_MESSAGE,
            };
          default:
            return {
              success: false,
              status: "pending",
              message: pendingMessage ?? DEFAULT_PENDING_MESSAGE,
            };
        }
      }

      const challenge = await createOperationChallenge({
        operationType,
        title,
        summary,
      });

      beginChallenge(challenge);

      if (challenge.grantToken) {
        setGrant({
          token: challenge.grantToken,
          operationType: challenge.operationType,
          expiresAt: challenge.grantExpiresAt ?? challenge.expiresAt ?? null,
        });
      }

      switch (challenge.status) {
        case "approved":
          if (!challenge.grantToken) {
            reset();

            return {
              success: false,
              status: "error",
              message: missingGrantMessage ?? DEFAULT_MISSING_GRANT_MESSAGE,
            };
          }

          return {
            success: true,
            status: "success",
            message: DEFAULT_SUCCESS_MESSAGE,
            token: challenge.grantToken,
          };
        case "rejected":
          reset();

          return {
            success: false,
            status: "error",
            message:
              challenge.rejectReason ??
              rejectedMessage ??
              DEFAULT_REJECTED_MESSAGE,
          };
        case "expired":
          reset();

          return {
            success: false,
            status: "error",
            message: expiredMessage ?? DEFAULT_EXPIRED_MESSAGE,
          };
        case "consumed":
          reset();

          return {
            success: false,
            status: "error",
            message: consumedMessage ?? DEFAULT_CONSUMED_MESSAGE,
          };
        default:
          return {
            success: true,
            status: "pending",
            message: startMessage ?? DEFAULT_START_MESSAGE,
            qrCodeUrl: challenge.qrCode,
          };
      }
    },
    [
      beginChallenge,
      consumedMessage,
      currentChallenge,
      expiredMessage,
      missingGrantMessage,
      operationType,
      pendingMessage,
      rejectedMessage,
      reset,
      resolveChallenge,
      setGrant,
      startMessage,
      summary,
      title,
      unsupportedMethodMessage,
    ],
  );

  return {
    currentChallenge,
    consumeGrant,
    reset,
    verify,
  };
}
