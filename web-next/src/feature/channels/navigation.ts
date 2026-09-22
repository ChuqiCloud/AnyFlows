import { defineNavigation } from "@/shared/navigation";

export const channelsNavNodes = defineNavigation(
  [
    {
      key: "channels",
      title: "nav.channels",
      href: "/console/channels",
      order: 30,
      section: "gateway",
      sectionTitle: "nav.sections.gateway",
    },
  ],
  { group: "module", module: "channels" },
);
