/**
 * 通用确认弹窗组件
 * 用于替代原生 confirm() 弹窗
 */

import {
  Button,
  Modal,
  ModalBody,
  ModalContent,
  ModalFooter,
  ModalHeader,
} from "@heroui/react";
import { Icon } from "@iconify/react";
import { useTranslation } from "react-i18next";

export interface ConfirmModalProps {
  isOpen: boolean;
  title?: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  confirmColor?:
    | "primary"
    | "danger"
    | "warning"
    | "success"
    | "secondary"
    | "default";
  icon?: string;
  iconColor?: string;
  isLoading?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmModal({
  isOpen,
  title,
  message,
  confirmText,
  cancelText,
  confirmColor = "danger",
  icon = "solar:danger-triangle-bold-duotone",
  iconColor = "text-warning-500",
  isLoading = false,
  onConfirm,
  onCancel,
}: ConfirmModalProps) {
  const { t } = useTranslation();
  return (
    <Modal
      classNames={{
        backdrop: "bg-black/50 backdrop-blur-sm",
      }}
      isOpen={isOpen}
      onClose={onCancel}
    >
      <ModalContent>
        <ModalHeader className="flex flex-col items-center gap-2 pt-6">
          <div className="flex h-14 w-14 items-center justify-center rounded-full bg-warning-100">
            <Icon className={iconColor} icon={icon} width={28} />
          </div>
          <h3 className="text-lg font-semibold">{title ?? t("commonUi.confirmTitle")}</h3>
        </ModalHeader>
        <ModalBody className="text-center px-6">
          <p className="text-default-600">{message}</p>
        </ModalBody>
        <ModalFooter className="justify-center gap-3 pb-6">
          <Button isDisabled={isLoading} variant="flat" onPress={onCancel}>
            {cancelText ?? t("commonUi.cancel")}
          </Button>
          <Button
            color={confirmColor}
            isLoading={isLoading}
            onPress={onConfirm}
          >
            {confirmText ?? t("commonUi.confirm")}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}

export default ConfirmModal;
