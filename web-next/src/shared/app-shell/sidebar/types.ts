import type { ReactNode, SVGProps } from "react";

export type SidebarNode = {
  key: string;
  title: string;
  icon?: string;
  href?: string;
  startContent?: ReactNode;
  endContent?: ReactNode;
  children?: SidebarNode[];
  className?: string;
};

export type SidebarProps = {
  items: SidebarNode[];
  isCompact?: boolean;
  hideEndContent?: boolean;
  iconClassName?: string;
  sectionClasses?: any;
  itemClasses?: any;
  classNames?: any;
  className?: string;
  defaultSelectedKey: string;
  onSelect?: (key: string) => void;
};

export type IconSvgProps = SVGProps<SVGSVGElement> & {
  size?: number;
};
