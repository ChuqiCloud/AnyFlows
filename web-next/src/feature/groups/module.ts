import { defineModule } from "@/shared/modules";

import { groupsNavNodes } from "./navigation";

const groupsModule = defineModule({
  id: "groups",
  order: 35,
  navigation: groupsNavNodes,
});

export { groupsModule };
export default groupsModule;
