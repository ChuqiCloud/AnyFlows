import type { NavLinkItem } from "../../types";

import { Button, cn, NavbarItem } from "@heroui/react";
import { Link as RouterLink, useLocation } from "react-router-dom";
import { Icon } from "@iconify/react";
import React from "react";

interface NavLinksProps {
  links: NavLinkItem[];
  className?: string;
}

/**
 * 导航链接组件
 */
export const NavLinks: React.FC<NavLinksProps> = ({ links, className }) => {
  const location = useLocation();

  return (
    <div className={cn("hidden lg:flex items-center gap-1", className)}>
      {links.map((link) => {
        const isActive = location.pathname.startsWith(link.href);

        return (
          <NavbarItem key={link.key}>
            <Button
              as={RouterLink}
              className={cn(
                "min-w-0 px-3 font-medium",
                isActive && "bg-default-100 text-primary",
              )}
              radius="full"
              size="sm"
              startContent={
                link.icon && (
                  <Icon
                    className="text-default-500"
                    icon={link.icon}
                    width={16}
                  />
                )
              }
              to={link.href}
              variant="light"
            >
              {link.label}
              {link.badge !== undefined && link.badge > 0 && (
                <span className="ml-1 px-1.5 py-0.5 text-tiny bg-danger text-white rounded-full">
                  {link.badge > 99 ? "99+" : link.badge}
                </span>
              )}
            </Button>
          </NavbarItem>
        );
      })}
    </div>
  );
};
