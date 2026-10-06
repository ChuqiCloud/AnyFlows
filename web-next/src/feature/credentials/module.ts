import { defineModule } from "@/shared/modules";

import { credentialsNavNodes } from "./navigation";

const credentialsModule = defineModule({
  id: "credentials",
  order: 40,
  navigation: credentialsNavNodes,
});

export { credentialsModule };
export default credentialsModule;
