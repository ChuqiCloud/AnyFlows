import { defineModule } from "@/shared/modules";

import { imagePlazaNavNodes } from "./navigation";

const imagePlazaModule = defineModule({
  id: "imagePlaza",
  order: 28,
  navigation: imagePlazaNavNodes,
});

export { imagePlazaModule };
export default imagePlazaModule;
