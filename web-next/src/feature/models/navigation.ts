import { defineNavigation } from "@/shared/navigation";

// 模型广场不再出现在工作台侧栏：用户从 Playground/视频生成内嵌的模型选择器进入。
// 这里仅保留站点顶部与产品入口的外部链接。
export const modelsNavNodes = defineNavigation(
  [
    {
      key: "top:models",
      title: "nav.models",
      href: "/models",
      icon: "solar:planet-2-linear",
      group: "top",
      order: 10,
    },
    {
      key: "product:models",
      title: "nav.models",
      description: "nav.products.models",
      href: "/models",
      icon: "solar:planet-2-bold",
      group: "product",
      order: 0,
      section: "platform",
      sectionTitle: "nav.products.section",
    },
  ],
  { group: "module", module: "models" },
);
