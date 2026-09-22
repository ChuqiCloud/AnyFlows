/**
 * 二次验证弹窗模块
 *
 * 提供全局复用的二次验证功能，支持以下验证方式：
 * - 短信验证
 * - 邮件验证
 * - 2FA验证
 * - 微信扫码授权
 *
 * @example
 * ```tsx
 * import { TwoFactorAuthModal, useTwoFactorAuth } from "@/shared/components/two-factor-auth";
 *
 * function MyComponent() {
 *   const { open, props } = useTwoFactorAuth({
 *     onVerify: async (request) => {
 *       // 处理验证逻辑
 *       const response = await api.verify(request);
 *       return response;
 *     },
 *     onSuccess: (response) => {
 *       console.log("验证成功", response);
 *     },
 *   });
 *
 *   return (
 *     <>
 *       <Button onPress={open}>打开验证</Button>
 *       <TwoFactorAuthModal {...props} />
 *     </>
 *   );
 * }
 * ```
 */

export { TwoFactorAuthModal } from "./components/containers/two-factor-auth-modal";
export { TwoFactorAuthModalContent } from "./components/presentation/two-factor-auth-modal-content";
export { useTwoFactorAuth } from "./hooks/use-two-factor-auth";
export * from "./types";
