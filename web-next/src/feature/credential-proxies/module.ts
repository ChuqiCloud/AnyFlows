import { defineModule } from "@/shared/modules";

import { credentialProxiesNavNodes } from "./navigation";

const credentialProxiesModule = defineModule({
  id: "credentialProxies",
  order: 50,
  navigation: credentialProxiesNavNodes,
});

export { credentialProxiesModule };
export default credentialProxiesModule;
