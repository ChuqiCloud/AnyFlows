import { defineModule } from "@/shared/modules";

import { announcementsNavNodes } from "./navigation";

const announcementsModule = defineModule({
  id: "announcements",
  order: 70,
  navigation: announcementsNavNodes,
});

export { announcementsModule };
export default announcementsModule;
