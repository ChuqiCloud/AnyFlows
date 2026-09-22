import { defineModule } from "@/shared/modules";

import { modelManagementNavNodes } from "./navigation";

const modelManagementModule = defineModule({
  id: "modelManagement",
  order: 25,
  navigation: modelManagementNavNodes,
});

export { modelManagementModule };
export default modelManagementModule;
