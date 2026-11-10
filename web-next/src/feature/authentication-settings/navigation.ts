import { defineNavigation } from "@/shared/navigation";

export const authenticationSettingsNavNodes = defineNavigation(
  [
    {
      key: "authenticationSettings",
      title: "nav.authenticationSettings",
      href: "/console/system-settings/authentication",
      order: 60,
      section: "platform",
      sectionTitle: "nav.sections.site",
    },
  ],
  { group: "module", module: "authenticationSettings" },
);
