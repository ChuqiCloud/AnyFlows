import {
  Button,
  cn,
  Divider,
  Drawer,
  DrawerBody,
  DrawerContent,
  DrawerHeader,
  Link,
} from "@heroui/react";
import { Icon } from "@iconify/react";
import { useTranslation } from "react-i18next";
import { Link as RouterLink, useLocation } from "react-router-dom";

import {
  groupNavigationNodesBySection,
  useNavigationNodes,
} from "@/shared/navigation";

interface ProductDrawerProps {
  isOpen: boolean;
  onClose: () => void;
}

export function ProductDrawer({ isOpen, onClose }: ProductDrawerProps) {
  const { t } = useTranslation();
  const location = useLocation();
  const productGroups = groupNavigationNodesBySection(
    useNavigationNodes({ group: "product" }),
  );

  return (
    <Drawer
      hideCloseButton
      isOpen={isOpen}
      placement="left"
      size="sm"
      onClose={onClose}
    >
      <DrawerContent>
        <DrawerHeader className="flex items-center justify-between border-b border-divider py-4">
          <h3 className="text-lg font-semibold text-foreground">
            {t("nav.products.section")}
          </h3>
          <Button
            isIconOnly
            aria-label={t("nav.products.close")}
            radius="full"
            size="sm"
            variant="light"
            onPress={onClose}
          >
            <Icon icon="solar:close-circle-linear" width={20} />
          </Button>
        </DrawerHeader>

        <DrawerBody className="gap-4 py-4">
          {productGroups.map((group, groupIndex) => (
            <section key={group.key}>
              <p className="mb-2 text-xs font-medium uppercase tracking-wide text-default-500">
                {group.title}
              </p>

              <div className="space-y-2">
                {group.items.map((item) => {
                  if (!item.href) {
                    return null;
                  }

                  const isActive = location.pathname.startsWith(item.href);

                  return (
                    <Link
                      key={item.key}
                      as={RouterLink}
                      className={cn(
                        "flex w-full items-center gap-3 rounded-xl border border-default-200 bg-content1 px-3 py-2.5 no-underline",
                        isActive && "border-primary/40 bg-primary/5",
                      )}
                      color="foreground"
                      to={item.href}
                      onPress={onClose}
                    >
                      <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-default-100">
                        <Icon
                          className="text-default-700"
                          icon={item.icon || "solar:widget-2-bold"}
                          width={16}
                        />
                      </div>

                      <div className="min-w-0 flex-1">
                        <p className="text-sm font-medium text-foreground">
                          {item.title}
                        </p>
                        <p className="line-clamp-1 text-xs text-default-500">
                          {item.description}
                        </p>
                      </div>

                      <Icon
                        className="text-default-400"
                        icon="solar:alt-arrow-right-linear"
                        width={16}
                      />
                    </Link>
                  );
                })}
              </div>

              {groupIndex < productGroups.length - 1 ? (
                <Divider className="mt-4" />
              ) : null}
            </section>
          ))}
        </DrawerBody>
      </DrawerContent>
    </Drawer>
  );
}
