import type {
  AuthMethodConfig,
  OnVerifyError,
  OnVerifySuccess,
  VerificationRequest,
  VerificationResponse,
} from "../types";

import { useCallback } from "react";
import { useDisclosure } from "@heroui/react";

interface UseTwoFactorAuthOptions {
  onVerify: (
    request: VerificationRequest,
  ) => Promise<VerificationResponse> | VerificationResponse;
  onSuccess?: OnVerifySuccess;
  onError?: OnVerifyError;
  availableMethods?: Partial<AuthMethodConfig>[];
  title?: string;
  description?: string;
}

interface UseTwoFactorAuthReturn {
  isOpen: boolean;
  open: () => void;
  close: () => void;
  toggle: () => void;
  onOpenChange: (open: boolean) => void;
  props: {
    isOpen: boolean;
    onOpenChange: (open: boolean) => void;
    availableMethods?: Partial<AuthMethodConfig>[];
    onVerify: (
      request: VerificationRequest,
    ) => Promise<VerificationResponse> | VerificationResponse;
    onSuccess?: OnVerifySuccess;
    onError?: OnVerifyError;
    title?: string;
    description?: string;
  };
}

/**
 * 二次验证弹窗 Hook
 *
 * @example
 * ```tsx
 * const { open, props } = useTwoFactorAuth({
 *   onVerify: async (request) => {
 *     // 处理验证逻辑
 *     return { success: true, token: "..." };
 *   },
 *   onSuccess: (response) => {
 *     console.log("验证成功", response);
 *   },
 * });
 *
 * return (
 *   <>
 *     <Button onPress={open}>打开验证</Button>
 *     <TwoFactorAuthModal {...props} />
 *   </>
 * );
 * ```
 */
export const useTwoFactorAuth = (
  options: UseTwoFactorAuthOptions,
): UseTwoFactorAuthReturn => {
  const { isOpen, onOpen, onClose, onOpenChange } = useDisclosure();

  const handleVerify = useCallback(
    async (request: VerificationRequest): Promise<VerificationResponse> => {
      try {
        const response = await options.onVerify(request);

        return response;
      } catch (error) {
        options.onError?.(
          error instanceof Error ? error : new Error(String(error)),
        );
        throw error;
      }
    },
    [options],
  );

  const handleSuccess = useCallback(
    (response: VerificationResponse) => {
      options.onSuccess?.(response);
    },
    [options],
  );

  const handleError = useCallback(
    (error: Error | string) => {
      options.onError?.(
        error instanceof Error ? error : new Error(String(error)),
      );
    },
    [options],
  );

  return {
    isOpen,
    open: onOpen,
    close: onClose,
    toggle: () => (isOpen ? onClose() : onOpen()),
    onOpenChange,
    props: {
      isOpen,
      onOpenChange,
      availableMethods: options.availableMethods,
      onVerify: handleVerify,
      onSuccess: handleSuccess,
      onError: handleError,
      title: options.title,
      description: options.description,
    },
  };
};
