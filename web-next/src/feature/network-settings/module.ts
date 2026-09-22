import { defineModule } from "@/shared/modules";

import { networkSettingsNavNodes } from "./navigation";

const networkSettingsModule = defineModule({
  id: "networkSettings",
  order: 65,
  navigation: networkSettingsNavNodes,
});

export { networkSettingsModule };
export default networkSettingsModule;
