import { defineNavigation } from "@/shared/navigation";

export const imagePlazaNavNodes = defineNavigation(
  [
    {
      key: "imagePlaza",
      title: "nav.imagePlaza",
      href: "/console/image-plaza",
      order: 8,
    },
  ],
  { group: "module", module: "imagePlaza", surface: "user" },
);
