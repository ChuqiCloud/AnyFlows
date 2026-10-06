import { defineNavigation } from "@/shared/navigation";

export const emailSettingsNavNodes = defineNavigation(
  [
    {
      key: "emailSettings",
      title: "nav.emailSettings",
      href: "/console/system-settings/email",
      order: 55,
      section: "platform",
      sectionTitle: "nav.sections.platform",
    },
  ],
  { group: "module", module: "emailSettings" },
);
