import { defineModule } from "@/shared/modules";

import { modelsNavNodes } from "./navigation";

const modelsModule = defineModule({
  id: "models",
  order: 6,
  navigation: modelsNavNodes,
});

export { modelsModule };
export default modelsModule;
