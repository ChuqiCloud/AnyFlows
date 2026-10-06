import { defineNavigation } from "@/shared/navigation";

export const billingSettingsNavNodes = defineNavigation(
  [
    {
      key: "billingSettings",
      title: "nav.billingSettings",
      href: "/console/system-settings/billing",
      order: 20,
      section: "operations",
      sectionTitle: "nav.sections.operations",
    },
  ],
  { group: "module", module: "billingSettings" },
);
