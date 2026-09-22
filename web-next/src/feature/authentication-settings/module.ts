import { defineModule } from "@/shared/modules";

import { authenticationSettingsNavNodes } from "./navigation";

const authenticationSettingsModule = defineModule({
  id: "authenticationSettings",
  order: 60,
  navigation: authenticationSettingsNavNodes,
});

export { authenticationSettingsModule };
export default authenticationSettingsModule;
