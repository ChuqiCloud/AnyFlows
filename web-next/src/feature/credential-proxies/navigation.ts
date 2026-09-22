import { defineNavigation } from "@/shared/navigation";

export const credentialProxiesNavNodes = defineNavigation(
  [
    {
      key: "credentialProxies",
      title: "nav.credentialProxies",
      href: "/console/proxies",
      order: 50,
      section: "gateway",
      sectionTitle: "nav.sections.gateway",
    },
  ],
  { group: "module", module: "credentialProxies" },
);
