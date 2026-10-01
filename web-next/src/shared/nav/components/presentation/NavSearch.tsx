import React, { useCallback, useState } from "react";
import {
  Button,
  cn,
  Input,
  Kbd,
  Modal,
  ModalBody,
  ModalContent,
} from "@heroui/react";
import { Icon } from "@iconify/react";
import { useTranslation } from "react-i18next";
import { navigateTo } from "@/lib/router-navigation";

interface NavSearchProps {
  className?: string;
}

/**
 * 导航搜索组件
 * - 桌面端：显示搜索框
 * - 移动端：显示搜索图标，点击打开搜索弹窗
 */
export const NavSearch: React.FC<NavSearchProps> = ({ className }) => {
  const { t } = useTranslation();
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [searchValue, setSearchValue] = useState("");

  const handleSearch = useCallback(() => {
    const query = searchValue.trim();
    if (!query) return;
    navigateTo(`/console/models?search=${encodeURIComponent(query)}`);
    setIsModalOpen(false);
  }, [searchValue]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        handleSearch();
      }
    },
    [handleSearch],
  );

  return (
    <>
      {/* 桌面端搜索框 */}
      <div className={cn("hidden lg:flex", className)}>
        <Input
          aria-label={t("shell.search")}
          classNames={{
            base: "max-w-[200px]",
            inputWrapper: cn(
              "shadow-none bg-default-100 dark:bg-default-50",
              "group-data-[focus=true]:bg-default-100",
              "dark:group-data-[focus=true]:bg-default-50",
            ),
          }}
          endContent={
            <Kbd className="hidden lg:inline-block shadow-none" keys={["command"]}>
              K
            </Kbd>
          }
          placeholder={t("nav.searchPlaceholder")}
          radius="full"
          size="sm"
          startContent={
            <Icon
              className="text-default-400"
              icon="solar:magnifer-linear"
              width={16}
            />
          }
          value={searchValue}
          onKeyDown={handleKeyDown}
          onValueChange={setSearchValue}
        />
      </div>

      {/* 移动端搜索按钮 */}
      <Button
        isIconOnly
        aria-label={t("shell.search")}
        className="lg:hidden"
        radius="full"
        size="sm"
        variant="light"
        onPress={() => setIsModalOpen(true)}
      >
        <Icon
          className="text-default-500"
          icon="solar:magnifer-linear"
          width={20}
        />
      </Button>

      {/* 移动端搜索弹窗 */}
      <Modal
        hideCloseButton
        classNames={{
          base: "m-0 rounded-none",
          body: "p-4",
        }}
        isOpen={isModalOpen}
        placement="top"
        size="full"
        onClose={() => setIsModalOpen(false)}
      >
        <ModalContent>
          <ModalBody>
            <Input
              autoFocus
              aria-label={t("shell.search")}
              endContent={
                <Button
                  isIconOnly
                  aria-label={t("nav.closeSearch")}
                  size="sm"
                  variant="light"
                  onPress={() => setIsModalOpen(false)}
                >
                  <Icon icon="solar:close-circle-linear" width={20} />
                </Button>
              }
              placeholder={t("nav.searchPlaceholder")}
              size="lg"
              startContent={
                <Icon
                  className="text-default-400"
                  icon="solar:magnifer-linear"
                  width={20}
                />
              }
              value={searchValue}
              onKeyDown={handleKeyDown}
              onValueChange={setSearchValue}
            />
          </ModalBody>
        </ModalContent>
      </Modal>
    </>
  );
};
