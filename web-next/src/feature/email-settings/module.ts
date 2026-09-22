import { defineModule } from "@/shared/modules";

import { emailSettingsNavNodes } from "./navigation";

const emailSettingsModule = defineModule({
  id: "emailSettings",
  order: 55,
  navigation: emailSettingsNavNodes,
});

export { emailSettingsModule };
export default emailSettingsModule;
