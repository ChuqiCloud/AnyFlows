import { defineModule } from "@/shared/modules";

import { billingSettingsNavNodes } from "./navigation";

const billingSettingsModule = defineModule({
  id: "billingSettings",
  order: 20,
  navigation: billingSettingsNavNodes,
});

export { billingSettingsModule };
export default billingSettingsModule;
