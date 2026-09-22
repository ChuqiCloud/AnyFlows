import { defineModule } from "@/shared/modules";

import { channelsNavNodes } from "./navigation";

const channelsModule = defineModule({
  id: "channels",
  order: 30,
  navigation: channelsNavNodes,
});

export { channelsModule };
export default channelsModule;
