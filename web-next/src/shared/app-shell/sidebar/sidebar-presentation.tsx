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
      defaultExpandedKeys,
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
                // 可折叠分组只是容器：HeroUI 选项默认的 hover 底色会铺满整块，压回透明，底色交给子项。
                { "h-auto p-0 data-[hover=true]:!bg-transparent data-[selectable=true]:focus:!bg-transparent": !isCompact && children.length > 0 },
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
              <Accordion
                key={`${item.key}:${defaultExpandedKeys?.join(",") ?? "all"}`}
                className={"p-0"}
                defaultExpandedKeys={defaultExpandedKeys === undefined ? "all" : defaultExpandedKeys.filter((key) => key === item.key)}
                // HeroUI Accordion 默认带 px-2 / 选中时还会被外层业务 CSS 加上侧栏主题色背景，
                // 侧栏场景下分组只承担"折叠展开"职责，整个块的底色交给外层卡片。
                isCompact
                variant="light"
              >
                <AccordionItem
                  key={item.key}
                  aria-label={item.title}
                  classNames={{
                    base: "!bg-transparent !shadow-none",
                    heading: "pr-3",
                    trigger: "p-0 !bg-transparent data-[hover=true]:!bg-transparent",
                    content: "py-0 pl-4 !bg-transparent",
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
                      (item.startContent ?? <span className="flex h-10 items-center px-2 text-small font-medium text-default-600">{item.title}</span>)
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
        defaultExpandedKeys,
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
