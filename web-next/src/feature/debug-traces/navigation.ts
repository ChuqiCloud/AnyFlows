import { defineNavigation } from "@/shared/navigation";

// 调试追踪在既有导航模型里属于平台管理分组，平台 section 词表尚无对应项，故不声明 section。
export const debugTracesNavNodes = defineNavigation(
  [
    {
      key: "debugTraces",
      title: "nav.debugTraces",
      href: "/console/debug-traces",
      order: 45,
    },
  ],
  { group: "module", module: "debugTraces" },
);
