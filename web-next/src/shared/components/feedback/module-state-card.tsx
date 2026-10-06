import type { ReactNode } from "react";

import { Button, Card, CardBody } from "@heroui/react";
import { Icon } from "@iconify/react";

type ModuleStateVariant = "error" | "empty" | "forbidden";

const variantStyles: Record<
  ModuleStateVariant,
  {
    cardClassName: string;
    icon: string;
    iconClassName: string;
    iconWrapperClassName: string;
    titleClassName: string;
    descriptionClassName: string;
  }
> = {
  error: {
    cardClassName: "border border-danger-200 bg-danger-50",
    icon: "solar:danger-triangle-linear",
    iconClassName: "text-danger-500",
    iconWrapperClassName: "bg-danger-100",
    titleClassName: "text-danger-700",
    descriptionClassName: "text-danger-600",
  },
  empty: {
    cardClassName: "border border-divider",
    icon: "solar:inbox-linear",
    iconClassName: "text-default-300",
    iconWrapperClassName: "bg-default-100",
    titleClassName: "text-default-700",
    descriptionClassName: "text-default-400",
  },
  forbidden: {
    cardClassName: "border border-warning-200 bg-warning-50",
    icon: "solar:lock-keyhole-minimalistic-linear",
    iconClassName: "text-warning-600",
    iconWrapperClassName: "bg-warning-100",
    titleClassName: "text-warning-700",
    descriptionClassName: "text-warning-600",
  },
};

export interface ModuleStateCardProps {
  variant: ModuleStateVariant;
  title: string;
  description: string;
  actionLabel?: string;
  onAction?: () => void;
  footer?: ReactNode;
  compact?: boolean;
}

export function ModuleStateCard({
  variant,
  title,
  description,
  actionLabel,
  onAction,
  footer,
  compact = false,
}: ModuleStateCardProps) {
  const styles = variantStyles[variant];

  return (
    <Card className={styles.cardClassName} shadow="none">
      {compact ? (
        <CardBody className="flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex items-start gap-3">
            <div
              className={`mt-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-xl ${styles.iconWrapperClassName}`}
            >
              <Icon className={styles.iconClassName} icon={styles.icon} width={18} />
            </div>
            <div>
              <p className={`text-sm font-semibold ${styles.titleClassName}`}>
                {title}
              </p>
              <p className={`text-sm ${styles.descriptionClassName}`}>
                {description}
              </p>
            </div>
          </div>
          {actionLabel && onAction ? (
            <Button size="sm" variant="flat" onPress={onAction}>
              {actionLabel}
            </Button>
          ) : footer ? (
            <div>{footer}</div>
          ) : null}
        </CardBody>
      ) : (
        <CardBody className="py-16">
          <div className="flex flex-col items-center justify-center text-center">
            <div
              className={`mb-4 flex h-16 w-16 items-center justify-center rounded-2xl ${styles.iconWrapperClassName}`}
            >
              <Icon className={styles.iconClassName} icon={styles.icon} width={32} />
            </div>
            <p className={`mb-1 text-base font-medium ${styles.titleClassName}`}>
              {title}
            </p>
            <p className={`max-w-xl text-small ${styles.descriptionClassName}`}>
              {description}
            </p>
            {actionLabel && onAction ? (
              <Button className="mt-5" size="sm" variant="flat" onPress={onAction}>
                {actionLabel}
              </Button>
            ) : null}
            {footer ? <div className="mt-4">{footer}</div> : null}
          </div>
        </CardBody>
      )}
    </Card>
  );
}
