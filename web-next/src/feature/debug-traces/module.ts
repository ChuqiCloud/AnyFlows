import { defineModule } from "@/shared/modules";

import { debugTracesNavNodes } from "./navigation";

const debugTracesModule = defineModule({
  id: "debugTraces",
  order: 45,
  navigation: debugTracesNavNodes,
});

export { debugTracesModule };
export default debugTracesModule;
