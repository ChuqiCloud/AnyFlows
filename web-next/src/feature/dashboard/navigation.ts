import { defineNavigation } from "@/shared/navigation";

/*
 * 控制台首页在既有导航模型里属于工作台分组，平台 section 词表尚无对应项，故不声明 section。
 * top / product 节点沿用 acmeidc 的导航契约：顶栏链接区与产品抽屉都从注册表读取，
 * 不注册这两组节点时 Header 会缺少链接、产品抽屉内容为空。
 */
export const dashboardNavNodes = defineNavigation(
  [
    {
      key: "overview",
      title: "nav.overview",
      href: "/console",
      order: 3,
    },
    {
      key: "top:console",
      title: "nav.console",
      href: "/console",
      icon: "solar:home-2-bold",
      group: "top",
      order: 0,
    },
    {
      key: "product:console",
      title: "nav.console",
      description: "nav.products.console",
      href: "/console",
      icon: "solar:widget-2-bold",
      group: "product",
      order: 10,
      section: "platform",
      sectionTitle: "nav.products.section",
    },
  ],
  { group: "module", module: "dashboard" },
);
