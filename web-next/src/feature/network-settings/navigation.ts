import { defineNavigation } from "@/shared/navigation";

export const networkSettingsNavNodes = defineNavigation(
  [
    {
      key: "networkSettings",
      title: "nav.networkSettings",
      href: "/console/system-settings/network",
      order: 65,
      section: "platform",
      sectionTitle: "nav.sections.site",
    },
  ],
  { group: "module", module: "networkSettings" },
);
