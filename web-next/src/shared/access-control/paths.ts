/** 控制台里普通用户即可访问的一级路径段；其余一律要求管理员。 */
const USER_ACCESSIBLE_SEGMENTS = new Set([
  "api-keys",
  "wallet",
  "subscriptions",
  "invitations",
  "models",
  "playground",
  "video-tasks",
  "usage-logs",
  "profile",
  "account-verification",
]);

/** 取 /console 之后的一级路径段，用于判定访问级别。 */
function consoleSegment(pathname: string) {
  const [head, segment] = pathname.replace(/^\//, "").split("/");

  return head === "console" ? (segment ?? "") : undefined;
}

/** 控制台默认要求管理员，只有用户工作台相关的少量页面向普通用户开放。 */
export function requiresAdmin(pathname: string) {
  const segment = consoleSegment(pathname);

  if (segment === undefined) return false;
  if (!segment) return true;

  return !USER_ACCESSIBLE_SEGMENTS.has(segment);
}
