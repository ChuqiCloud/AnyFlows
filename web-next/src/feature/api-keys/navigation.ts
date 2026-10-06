import { defineNavigation } from "@/shared/navigation";

export const apiKeysNavNodes = defineNavigation(
  [
    {
      key: "apiKeys",
      title: "nav.apiKeys",
      href: "/console/api-keys",
      order: 10,
      section: "personal",
      sectionTitle: "nav.groups.personal",
    },
  ],
  { group: "module", module: "apiKeys" },
);
