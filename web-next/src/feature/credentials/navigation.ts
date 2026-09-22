import { defineNavigation } from "@/shared/navigation";

export const credentialsNavNodes = defineNavigation(
  [
    {
      key: "credentials",
      title: "nav.credentials",
      href: "/console/credentials",
      order: 40,
      section: "gateway",
      sectionTitle: "nav.sections.gateway",
    },
  ],
  { group: "module", module: "credentials" },
);
