import { defineModule } from "@/shared/modules";

// 登录、找回密码等公开入口没有侧边栏条目，因此不声明 navigation。
const authModule = defineModule({
  id: "auth",
  order: 1,
});

export { authModule };
export default authModule;
