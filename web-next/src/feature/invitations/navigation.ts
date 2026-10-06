import { defineNavigation } from "@/shared/navigation";

export const invitationsNavNodes = defineNavigation(
  [
    {
      key: "invitations",
      title: "nav.invitations",
      href: "/console/invitations",
      order: 15,
      section: "personal",
      sectionTitle: "nav.sections.personal",
    },
  ],
  { group: "module", module: "invitations" },
);
