import type {
  AuthMethodConfig,
  TwoFactorAuthMethod,
  VerificationStatus,
} from "../../types";

import { useEffect, useState } from "react";
import {
  Button,
  Card,
  CardBody,
  Input,
  Spinner,
  Tab,
  Tabs,
} from "@heroui/react";
import { Icon as IconifyIcon } from "@iconify/react";

interface TwoFactorAuthModalContentProps {
  selectedMethod: TwoFactorAuthMethod;
  availableMethods: AuthMethodConfig[];
  verificationStatus: VerificationStatus;
  errorMessage?: string;
  countdown?: number;
  qrCodeUrl?: string;
  onMethodChange: (method: TwoFactorAuthMethod) => void;
  onSendCode: (method: TwoFactorAuthMethod) => void;
  onVerify: (method: TwoFactorAuthMethod, code: string) => void;
  onCancel: () => void;
}

/** 各验证方式的描述信息 */
const methodHints: Record<TwoFactorAuthMethod, string> = {
  sms: "验证码会以短信发到你的手机",
  email: "验证码会发到你的邮箱",
  totp: "打开身份验证器 App，输入当前显示的 6 位验证码",
  wechat: "用微信扫下方二维码完成验证",
};

export const TwoFactorAuthModalContent = ({
  selectedMethod,
  availableMethods,
  verificationStatus,
  errorMessage,
  countdown,
  qrCodeUrl,
  onMethodChange,
  onSendCode,
  onVerify,
  onCancel,
}: TwoFactorAuthModalContentProps) => {
  const [code, setCode] = useState("");

  // 切换验证方式时清空输入
  useEffect(() => {
    setCode("");
  }, [selectedMethod]);

  const handleVerify = () => {
    if (code.trim()) {
      onVerify(selectedMethod, code.trim());
    }
  };

  const handleSendCode = () => {
    onSendCode(selectedMethod);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && code.trim().length > 0 && !isVerifying) {
      handleVerify();
    }
  };

  const isCodeRequired = selectedMethod !== "wechat";
  const needsSendCode = selectedMethod === "sms" || selectedMethod === "email";
  const isSending = verificationStatus === "sending";
  const isVerifying = verificationStatus === "verifying";
  const isSuccess = verificationStatus === "success";
  const canSendCode =
    !isSending && (countdown === undefined || countdown === 0);
  const canVerify = code.trim().length > 0 && !isVerifying;

  return (
    <div className="w-full space-y-6">
      {/* 验证方式选择 */}
      <Tabs
        aria-label="验证方式选择"
        classNames={{
          tabList: "w-full gap-4",
          tab: "flex-1",
          cursor: "w-full",
        }}
        selectedKey={selectedMethod}
        variant="underlined"
        onSelectionChange={(key) => onMethodChange(key as TwoFactorAuthMethod)}
      >
        {availableMethods.map((method) => (
          <Tab
            key={method.method}
            isDisabled={!method.enabled}
            title={
              <div className="flex items-center gap-2">
                <IconifyIcon icon={method.icon} width={16} />
                <span>{method.label}</span>
              </div>
            }
          />
        ))}
      </Tabs>

      {/* 验证内容卡片 */}
      <Card className="border border-divider" shadow="none">
        <CardBody className="p-5 space-y-5">
          {/* 验证方式描述 */}
          <p className="text-small text-default-500">
            {methodHints[selectedMethod]}
          </p>

          {/* 验证内容区域 */}
          {isSuccess ? (
            /* 验证成功状态 */
            <div className="flex flex-col items-center py-4">
              <div className="w-14 h-14 rounded-full bg-success/10 flex items-center justify-center mb-3">
                <IconifyIcon
                  className="text-success"
                  icon="solar:check-circle-bold"
                  width={28}
                />
              </div>
              <p className="text-sm font-medium text-success">验证成功</p>
            </div>
          ) : selectedMethod === "wechat" ? (
            /* 微信扫码 */
            <div className="flex flex-col items-center py-2">
              {qrCodeUrl ? (
                <>
                  <div className="bg-white p-3 rounded-xl border border-default-200 mb-3">
                    <img
                      alt="微信扫码二维码"
                      className="w-44 h-44"
                      src={qrCodeUrl}
                    />
                  </div>
                  <div className="flex items-center gap-2 text-tiny text-default-400">
                    <Spinner size="sm" />
                    <span>等待扫码确认中...</span>
                  </div>
                </>
              ) : (
                <div className="flex flex-col items-center gap-3 py-8">
                  <Spinner size="lg" />
                  <p className="text-sm text-default-500">正在生成二维码...</p>
                </div>
              )}
            </div>
          ) : (
            /* 验证码输入 */
            <div className="space-y-4">
              {/* 发送验证码按钮（短信/邮箱需要） */}
              {needsSendCode && (
                <Button
                  fullWidth
                  isDisabled={!canSendCode}
                  isLoading={isSending}
                  startContent={
                    !isSending && (
                      <IconifyIcon
                        icon={
                          countdown && countdown > 0
                            ? "solar:clock-circle-linear"
                            : "solar:letter-linear"
                        }
                        width={18}
                      />
                    )
                  }
                  variant="bordered"
                  onPress={handleSendCode}
                >
                  {countdown && countdown > 0
                    ? `${countdown}s 后可重新发送`
                    : "发送验证码"}
                </Button>
              )}

              <Input
                autoFocus={!needsSendCode}
                classNames={{
                  input: "text-center text-lg tracking-[0.3em] font-mono",
                }}
                errorMessage={errorMessage}
                isDisabled={isVerifying}
                isInvalid={!!errorMessage}
                label="验证码"
                maxLength={6}
                placeholder="请输入 6 位验证码"
                type="text"
                value={code}
                onKeyDown={handleKeyDown}
                onValueChange={setCode}
              />
            </div>
          )}
        </CardBody>
      </Card>

      {/* 操作按钮 */}
      <div className="flex gap-3 justify-end">
        <Button isDisabled={isVerifying} variant="light" onPress={onCancel}>
          取消
        </Button>
        {isCodeRequired && !isSuccess && (
          <Button
            color="primary"
            isDisabled={!canVerify}
            isLoading={isVerifying}
            onPress={handleVerify}
          >
            验证
          </Button>
        )}
      </div>
    </div>
  );
};
