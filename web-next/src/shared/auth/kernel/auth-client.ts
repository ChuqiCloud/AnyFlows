import { navigateTo } from "@/lib/router-navigation";

const DEFAULT_RETURN_PATH = "/console";

/** 未登录访问受保护路由时统一跳转的登录页。 */
export const LOGIN_PATH = "/login";

/** 需要登录的路径前缀；AnyFlows 只有控制台受保护，其余（模型广场、分享页等）都是公开页。 */
const PROTECTED_PATH_PREFIXES = ["/console"];

export function isProtectedPath(pathname: string): boolean {
  return PROTECTED_PATH_PREFIXES.some(
    (prefix) => pathname === prefix || pathname.startsWith(`${prefix}/`),
  );
}

function isAuthPath(pathname: string): boolean {
  return pathname === LOGIN_PATH || pathname.startsWith(`${LOGIN_PATH}/`);
}

export function sanitizeReturnPath(returnPath?: string | null): string {
  if (!returnPath) {
    return DEFAULT_RETURN_PATH;
  }

  try {
    const url = new URL(returnPath, window.location.origin);

    if (url.origin !== window.location.origin) {
      return DEFAULT_RETURN_PATH;
    }

    const normalizedPath = `${url.pathname}${url.search}${url.hash}`;

    if (!normalizedPath.startsWith("/") || isAuthPath(url.pathname)) {
      return DEFAULT_RETURN_PATH;
    }

    return normalizedPath || DEFAULT_RETURN_PATH;
  } catch {
    return DEFAULT_RETURN_PATH;
  }
}

export function getCurrentReturnPath(): string {
  return sanitizeReturnPath(
    `${window.location.pathname}${window.location.search}${window.location.hash}`,
  );
}

/** 登录地址：非默认页会带上原路径，登录后可以直接回到原页面。 */
export function resolveLoginPath(returnPath?: string | null): string {
  const from = sanitizeReturnPath(returnPath ?? getCurrentReturnPath());

  if (from === DEFAULT_RETURN_PATH) {
    return LOGIN_PATH;
  }

  return `${LOGIN_PATH}?from=${encodeURIComponent(from)}`;
}

/** 未登录时跳转登录页；站内跳转交给路由，保留 SPA 体验。 */
export async function redirectToAuth(returnPath?: string): Promise<void> {
  navigateTo(resolveLoginPath(returnPath), { replace: true });
}
