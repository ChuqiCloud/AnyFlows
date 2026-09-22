import { defineModule } from "@/shared/modules";

import { invitationsNavNodes } from "./navigation";

const invitationsModule = defineModule({
  id: "invitations",
  order: 15,
  navigation: invitationsNavNodes,
});

export { invitationsModule };
export default invitationsModule;
