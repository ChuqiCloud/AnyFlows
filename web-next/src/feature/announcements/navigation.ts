import { defineNavigation } from "@/shared/navigation";

export const announcementsNavNodes = defineNavigation(
  [
    {
      key: "announcements",
      title: "nav.announcements",
      href: "/console/system-settings/announcements",
      order: 70,
      section: "platform",
      sectionTitle: "nav.sections.platform",
    },
  ],
  { group: "module", module: "announcements" },
);
