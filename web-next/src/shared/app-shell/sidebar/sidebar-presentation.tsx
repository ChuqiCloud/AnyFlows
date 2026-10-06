import type { SidebarNode, SidebarProps } from "./types";

import {
  Accordion,
  AccordionItem,
  cn,
  Listbox,
  ListboxItem,
  type ListboxProps,
  ListboxSection,
  type Selection,
} from "@heroui/react";
import React from "react";
import { useTranslation } from "react-i18next";

import { SiteTooltip } from "@/shared/components/site-tooltip";
import { getSidebarNodeChildren } from "./tree";
import { SidebarIcon } from "./sidebar-icon";

type SidebarPresentationProps = Omit<
  SidebarProps,
  "defaultSelectedKey" | "onSelect"
> & {
  selected: React.Key;
  onSelectionChange: (keys: Selection) => void;
} & Omit<
    ListboxProps<SidebarNode>,
    "children" | "items" | "selectedKeys" | "onSelectionChange"
  >;

const stripHref = ({ href, ...rest }: SidebarNode) => rest;

export const SidebarPresentation = React.forwardRef<
  HTMLElement,
  SidebarPresentationProps
>(
  (
    {
      items,
      isCompact,
      hideEndContent,
      sectionClasses: sectionClassesProp = {},
      itemClasses: itemClassesProp = {},
      iconClassName,
      classNames,
      className,
      selected,
      onSelectionChange,
      ...props
    },
    ref,
  ) => {
    const { t } = useTranslation();
    const sectionClasses = {
      ...sectionClassesProp,
      base: cn(sectionClassesProp?.base, "w-full", {
        "p-0 max-w-[44px]": isCompact,
      }),
      group: cn(sectionClassesProp?.group, {
        "flex flex-col gap-1": isCompact,
      }),
      heading: cn(sectionClassesProp?.heading, {
        hidden: isCompact,
      }),
    };

    const itemClasses = {
      ...itemClassesProp,
      base: cn(itemClassesProp?.base, {
        "w-11 h-11 gap-0 p-0": isCompact,
      }),
    };

    const renderIcon = (icon: string) => (
      <SidebarIcon
        className={cn(
          "text-default-400 group-data-[selected=true]:text-primary",
          iconClassName,
        )}
        icon={icon}
        width={20}
      />
    );

    const renderNestItem = React.useCallback(
      (item: SidebarNode) => {
        const children = getSidebarNodeChildren(item);

        return (
          <ListboxItem
            {...stripHref(item)}
            key={item.key}
            classNames={{
              base: cn(
                { "h-auto p-0": !isCompact && children.length > 0 },
                { "inline-block w-11": isCompact && children.length > 0 },
              ),
            }}
            endContent={
              isCompact || children.length > 0 || hideEndContent
                ? null
                : (item.endContent ?? null)
            }
            startContent={
              isCompact || children.length > 0
                ? null
                : item.icon
                  ? renderIcon(item.icon)
                  : (item.startContent ?? null)
            }
            textValue={item.title}
            title={isCompact || children.length > 0 ? null : item.title}
          >
            {isCompact ? (
              <SiteTooltip content={item.title} placement="right">
                <div className="flex w-full items-center justify-center">
                  {item.icon
                    ? renderIcon(item.icon)
                    : (item.startContent ?? null)}
                </div>
              </SiteTooltip>
            ) : null}
            {!isCompact && children.length > 0 ? (
              <Accordion className={"p-0"}>
                <AccordionItem
                  key={item.key}
                  aria-label={item.title}
                  classNames={{
                    heading: "pr-3",
                    trigger: "p-0",
                    content: "py-0 pl-4",
                  }}
                  title={
                    item.icon ? (
                      <div
                        className={"flex h-10 items-center gap-2 px-2 py-1.5"}
                      >
                        {renderIcon(item.icon)}
                        <span className="text-small text-default-600 group-data-[selected=true]:text-primary font-medium">
                          {item.title}
                        </span>
                      </div>
                    ) : (
                      (item.startContent ?? null)
                    )
                  }
                >
                  {children.length > 0 ? (
                    <Listbox
                      disallowEmptySelection
                      aria-label={item.title}
                      className={"mt-0.5"}
                      classNames={{
                        list: cn("border-l border-default-200 pl-4"),
                        ...(classNames ?? {}),
                      }}
                      color="default"
                      itemClasses={{
                        ...itemClasses,
                        base: cn(
                          "px-3 min-h-10 rounded-full h-10 data-[selected=true]:bg-sidebar-accent! data-[hover=true]:bg-foreground/8",
                          itemClasses?.base,
                        ),
                        title: cn(
                          "text-small text-default-600 group-data-[selected=true]:text-primary group-data-[selected=true]:font-semibold",
                          itemClasses?.title,
                        ),
                      }}
                      items={children}
                      selectedKeys={[selected] as unknown as Selection}
                      selectionMode="single"
                      variant="flat"
                      onSelectionChange={onSelectionChange}
                    >
                      {children.map(renderItem)}
                    </Listbox>
                  ) : (
                    renderItem(item)
                  )}
                </AccordionItem>
              </Accordion>
            ) : null}
          </ListboxItem>
        );
      },
      [
        isCompact,
        hideEndContent,
        iconClassName,
        items,
        selected,
        onSelectionChange,
        itemClasses,
        classNames,
      ],
    );

    const renderItem = React.useCallback(
      (item: SidebarNode) => {
        const children = getSidebarNodeChildren(item);

        if (children.length > 0) {
          return renderNestItem(item);
        }

        return (
          <ListboxItem
            {...stripHref(item)}
            key={item.key}
            endContent={
              isCompact || hideEndContent ? null : (item.endContent ?? null)
            }
            startContent={
              isCompact
                ? null
                : item.icon
                  ? renderIcon(item.icon)
                  : (item.startContent ?? null)
            }
            textValue={item.title}
            title={isCompact ? null : item.title}
          >
            {isCompact ? (
              <SiteTooltip content={item.title} placement="right">
                <div className="flex w-full items-center justify-center">
                  {item.icon
                    ? renderIcon(item.icon)
                    : (item.startContent ?? null)}
                </div>
              </SiteTooltip>
            ) : null}
          </ListboxItem>
        );
      },
      [isCompact, hideEndContent, iconClassName, renderNestItem],
    );

    return (
      <Listbox
        key={isCompact ? "compact" : "default"}
        ref={ref}
        disallowEmptySelection
        hideSelectedIcon
        aria-label={t("nav.label")}
        as="nav"
        className={cn("list-none", className)}
        classNames={{
          ...classNames,
          list: cn("items-center gap-1", classNames?.list),
        }}
        color="default"
        itemClasses={{
          ...itemClasses,
          base: cn(
            "px-3 min-h-10 rounded-full h-10 data-[selected=true]:bg-sidebar-accent! data-[hover=true]:bg-foreground/8",
            itemClasses?.base,
          ),
          title: cn(
            "text-small text-default-600 group-data-[selected=true]:text-primary group-data-[selected=true]:font-semibold",
            itemClasses?.title,
          ),
        }}
        items={items}
        selectedKeys={[selected] as unknown as Selection}
        selectionMode="single"
        variant="flat"
        onSelectionChange={onSelectionChange}
        {...props}
      >
        {(item) => {
          const children = getSidebarNodeChildren(item);

          return children.length > 0 ? (
            <ListboxSection
              key={item.key}
              classNames={sectionClasses}
              showDivider={isCompact}
              title={item.title}
            >
              {children.map(renderItem)}
            </ListboxSection>
          ) : (
            renderItem(item)
          );
        }}
      </Listbox>
    );
  },
);

SidebarPresentation.displayName = "SidebarPresentation";
