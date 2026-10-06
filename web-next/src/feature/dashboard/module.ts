import { defineModule } from "@/shared/modules";

import { dashboardNavNodes } from "./navigation";

const dashboardModule = defineModule({
  id: "dashboard",
  order: 3,
  navigation: dashboardNavNodes,
});

export { dashboardModule };
export default dashboardModule;
