import type {
  AuthMethodConfig,
  OnVerifyCallback,
  OnVerifyError,
  OnVerifySuccess,
  TwoFactorAuthMethod,
  VerificationStatus,
} from "../../types";

import { useCallback, useEffect, useState } from "react";
import { Modal, ModalBody, ModalContent, ModalHeader } from "@heroui/react";
import { Icon } from "@iconify/react";

import { TwoFactorAuthModalContent } from "../presentation/two-factor-auth-modal-content";

interface TwoFactorAuthModalProps {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  availableMethods?: Partial<AuthMethodConfig>[];
  onVerify: OnVerifyCallback;
  onSuccess?: OnVerifySuccess;
  onError?: OnVerifyError;
  title?: string;
  description?: string;
}

const defaultMethods: AuthMethodConfig[] = [
  {
    method: "sms",
    label: "短信",
    icon: "solar:phone-calling-linear",
    enabled: true,
  },
  {
    method: "email",
    label: "邮件",
    icon: "solar:letter-linear",
    enabled: true,
  },
  {
    method: "totp",
    label: "2FA",
    icon: "solar:shield-check-linear",
    enabled: true,
  },
  {
    method: "wechat",
    label: "微信扫码",
    icon: "solar:qr-code-linear",
    enabled: true,
  },
];

export const TwoFactorAuthModal = ({
  isOpen,
  onOpenChange,
  availableMethods = defaultMethods,
  onVerify,
  onSuccess,
  onError,
  title = "二次验证",
  description,
}: TwoFactorAuthModalProps) => {
  const [selectedMethod, setSelectedMethod] =
    useState<TwoFactorAuthMethod>("sms");
  const [verificationStatus, setVerificationStatus] =
    useState<VerificationStatus>("idle");
  const [errorMessage, setErrorMessage] = useState<string | undefined>();
  const [countdown, setCountdown] = useState<number | undefined>();
  const [qrCodeUrl, setQrCodeUrl] = useState<string | undefined>();

  // 合并默认方法和用户提供的方法配置
  const methods: AuthMethodConfig[] = defaultMethods.map((defaultMethod) => {
    const userMethod = availableMethods.find(
      (m) => m.method === defaultMethod.method,
    );

    return userMethod ? { ...defaultMethod, ...userMethod } : defaultMethod;
  });

  // 倒计时逻辑
  useEffect(() => {
    if (countdown && countdown > 0) {
      const timer = setTimeout(() => {
        setCountdown(countdown - 1);
      }, 1000);

      return () => clearTimeout(timer);
    }
  }, [countdown]);

  // 重置状态
  const resetState = useCallback(() => {
    setVerificationStatus("idle");
    setErrorMessage(undefined);
    setCountdown(undefined);
    setQrCodeUrl(undefined);
  }, []);

  // 当弹窗关闭时重置状态
  useEffect(() => {
    if (!isOpen) {
      resetState();
    }
  }, [isOpen, resetState]);

  // 当切换验证方式时重置状态
  useEffect(() => {
    resetState();
  }, [selectedMethod, resetState]);

  // 轮询检查微信扫码状态
  const startPolling = useCallback(
    async (method: TwoFactorAuthMethod) => {
      const pollInterval = setInterval(async () => {
        try {
          const response = await onVerify({ method });

          if (response.success && response.token) {
            clearInterval(pollInterval);
            setVerificationStatus("success");
            onSuccess?.(response);
            setTimeout(() => {
              onOpenChange(false);
            }, 500);
          } else if (response.message && response.message !== "等待扫码确认") {
            setErrorMessage(response.message);
          }
        } catch (error) {
          const errorMsg =
            error instanceof Error ? error.message : "扫码状态检查失败";

          setErrorMessage(errorMsg);
          onError?.(error instanceof Error ? error : new Error(errorMsg));
        }
      }, 2000);

      setTimeout(() => {
        clearInterval(pollInterval);
      }, 30000);
    },
    [onVerify, onSuccess, onOpenChange],
  );

  // 发送验证码
  const handleSendCode = useCallback(
    async (method: TwoFactorAuthMethod) => {
      setVerificationStatus("sending");
      setErrorMessage(undefined);

      try {
        const response = await onVerify({ method });

        if (response.success) {
          setCountdown(60);
          setVerificationStatus("idle");

          if (method === "wechat" && response.qrCodeUrl) {
            setQrCodeUrl(response.qrCodeUrl);
            startPolling(method);
          }
        } else {
          setErrorMessage(response.message || "发送验证码失败");
          setVerificationStatus("error");
          onError?.(new Error(response.message || "发送验证码失败"));
        }
      } catch (error) {
        const errorMsg =
          error instanceof Error ? error.message : "发送验证码失败";

        setErrorMessage(errorMsg);
        setVerificationStatus("error");
        onError?.(error instanceof Error ? error : new Error(errorMsg));
      }
    },
    [onVerify, onError, startPolling],
  );

  // 验证
  const handleVerify = useCallback(
    async (method: TwoFactorAuthMethod, code: string) => {
      setVerificationStatus("verifying");
      setErrorMessage(undefined);

      try {
        const response = await onVerify({ method, code });

        if (response.success) {
          setVerificationStatus("success");
          onSuccess?.(response);
          setTimeout(() => {
            onOpenChange(false);
          }, 500);
        } else {
          setErrorMessage(response.message || "验证失败");
          setVerificationStatus("error");
          onError?.(new Error(response.message || "验证失败"));
        }
      } catch (error) {
        const errorMsg = error instanceof Error ? error.message : "验证失败";

        setErrorMessage(errorMsg);
        setVerificationStatus("error");
        onError?.(error instanceof Error ? error : new Error(errorMsg));
      }
    },
    [onVerify, onSuccess, onError, onOpenChange],
  );

  // 处理验证方式切换
  const handleMethodChange = useCallback(
    (method: TwoFactorAuthMethod) => {
      setSelectedMethod(method);
      if (method === "wechat") {
        handleSendCode(method);
      }
    },
    [handleSendCode],
  );

  // 处理取消
  const handleCancel = useCallback(() => {
    onOpenChange(false);
  }, [onOpenChange]);

  // 确保选中的方法可用
  const enabledMethods = methods.filter((m) => m.enabled);
  const currentMethod = methods.find((m) => m.method === selectedMethod);
  const finalSelectedMethod = currentMethod?.enabled
    ? selectedMethod
    : enabledMethods[0]?.method || "sms";

  useEffect(() => {
    if (isOpen && selectedMethod !== finalSelectedMethod) {
      setSelectedMethod(finalSelectedMethod);
    }
  }, [finalSelectedMethod, isOpen, selectedMethod]);

  useEffect(() => {
    if (
      isOpen &&
      finalSelectedMethod === "wechat" &&
      !qrCodeUrl &&
      verificationStatus === "idle"
    ) {
      handleSendCode("wechat");
    }
  }, [
    finalSelectedMethod,
    handleSendCode,
    isOpen,
    qrCodeUrl,
    verificationStatus,
  ]);

  return (
    <Modal
      isDismissable={verificationStatus !== "verifying"}
      isKeyboardDismissDisabled={verificationStatus === "verifying"}
      isOpen={isOpen}
      scrollBehavior="inside"
      size="md"
      onOpenChange={onOpenChange}
    >
      <ModalContent>
        {() => (
          <>
            <ModalHeader>
              <div className="flex items-center gap-2">
                <Icon
                  className="text-secondary"
                  icon="solar:shield-keyhole-bold"
                  width={24}
                />
                <div>
                  <h2 className="text-lg font-semibold">{title}</h2>
                  {description && (
                    <p className="text-sm text-default-500 font-normal">
                      {description}
                    </p>
                  )}
                </div>
              </div>
            </ModalHeader>
            <ModalBody className="pb-6">
              <TwoFactorAuthModalContent
                availableMethods={enabledMethods}
                countdown={countdown}
                errorMessage={errorMessage}
                qrCodeUrl={qrCodeUrl}
                selectedMethod={finalSelectedMethod}
                verificationStatus={verificationStatus}
                onCancel={handleCancel}
                onMethodChange={handleMethodChange}
                onSendCode={handleSendCode}
                onVerify={handleVerify}
              />
            </ModalBody>
          </>
        )}
      </ModalContent>
    </Modal>
  );
};
