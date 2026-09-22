import { defineNavigation } from "@/shared/navigation";

export const groupsNavNodes = defineNavigation(
  [
    {
      key: "groupSettings",
      title: "nav.groupSettings",
      href: "/console/system-settings/groups",
      order: 35,
      section: "gateway",
      sectionTitle: "nav.sections.gateway",
    },
  ],
  { group: "module", module: "groups" },
);
