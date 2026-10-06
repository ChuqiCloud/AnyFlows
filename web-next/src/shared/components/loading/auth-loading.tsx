import { Spinner } from "@heroui/react";
import { useTranslation } from "react-i18next";

export const AuthLoading = () => {
  const { t } = useTranslation();
  return (
    <div className="flex items-center justify-center min-h-screen w-full bg-gradient-to-br from-background to-default-100">
      <div className="flex flex-col items-center gap-6">
        <Spinner
          classNames={{
            label: "text-default-600 text-sm mt-2",
          }}
          color="primary"
          label={t("commonUi.authLoading")}
          size="lg"
          variant="gradient"
        />
      </div>
    </div>
  );
};
