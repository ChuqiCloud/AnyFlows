import { defineNavigation } from "@/shared/navigation";

// 模型广场在既有导航模型里属于工作台分组，平台 section 词表尚无对应项，故不声明 section。
export const modelsNavNodes = defineNavigation(
  [
    {
      key: "models",
      title: "nav.models",
      href: "/console/models",
      order: 6,
    },
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
