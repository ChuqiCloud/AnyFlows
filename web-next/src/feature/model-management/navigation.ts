import { defineNavigation } from "@/shared/navigation";

export const modelManagementNavNodes = defineNavigation(
  [
    {
      key: "modelSettings",
      title: "nav.modelSettings",
      href: "/console/system-settings/models",
      order: 25,
      section: "gateway",
      sectionTitle: "nav.sections.gateway",
    },
  ],
  { group: "module", module: "modelManagement" },
);
