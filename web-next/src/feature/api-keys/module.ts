import { defineModule } from "@/shared/modules";

import { apiKeysNavNodes } from "./navigation";

const apiKeysModule = defineModule({
  id: "apiKeys",
  order: 10,
  navigation: apiKeysNavNodes,
});

export { apiKeysModule };
export default apiKeysModule;
