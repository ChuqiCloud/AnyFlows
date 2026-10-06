import { Spinner } from "@heroui/react";
import { useTranslation } from "react-i18next";

export const PageLoading = () => {
  const { t } = useTranslation();
  return (
    <div className="flex items-center justify-center min-h-screen w-full">
      <Spinner
        classNames={{
          label: "text-default-500 text-sm mt-2",
        }}
        color="primary"
        label={t("commonUi.pageLoading")}
        size="lg"
        variant="dots"
      />
    </div>
  );
};
