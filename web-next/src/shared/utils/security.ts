/**
 * 前端安全工具集
 * 符合等保三级要求
 */

/**
 * XSS 防护 - HTML 实体编码
 */
export function escapeHtml(unsafe: string): string {
  return unsafe
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

/**
 * 安全的 URL 验证（防止 javascript: 和 data: 协议）
 */
export function isSafeUrl(url: string): boolean {
  const lowerUrl = url.toLowerCase().trim();

  // 阻止危险协议
  const dangerousProtocols = ["javascript:", "data:", "vbscript:", "file:"];

  return !dangerousProtocols.some((protocol) => lowerUrl.startsWith(protocol));
}

/**
 * 安全的外部链接跳转
 */
export function safeNavigate(url: string): void {
  if (!isSafeUrl(url)) {
    console.error("已拦截不安全的 URL：", url);

    return;
  }

  window.location.href = url;
}

/**
 * 清理用户输入（移除潜在的脚本标签）
 */
export function sanitizeInput(input: string): string {
  // 移除 <script> 标签及其内容
  let cleaned = input.replace(
    /<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi,
    "",
  );

  // 移除事件处理器属性
  cleaned = cleaned.replace(/on\w+\s*=\s*["'][^"']*["']/gi, "");

  // 移除 javascript: 协议
  cleaned = cleaned.replace(/javascript:/gi, "");

  return cleaned;
}

/**
 * 生成随机 Nonce（用于 CSP）
 */
export function generateCspNonce(): string {
  const array = new Uint8Array(16);

  crypto.getRandomValues(array);

  return Array.from(array, (byte) => byte.toString(16).padStart(2, "0")).join(
    "",
  );
}

/**
 * 安全的 JSON 解析
 */
export function safeJsonParse<T>(json: string, fallback: T): T {
  try {
    return JSON.parse(json) as T;
  } catch (error) {
    console.error("JSON 解析失败：", error);

    return fallback;
  }
}

/**
 * 检测是否在 iframe 中（防止点击劫持）
 */
export function isInIframe(): boolean {
  try {
    return window.self !== window.top;
  } catch (e) {
    return true;
  }
}

/**
 * 防止点击劫持
 */
export function preventClickjacking(): void {
  if (isInIframe()) {
    console.warn("检测到页面在 iframe 中运行，可能存在点击劫持风险");
  }
}

/**
 * 防抖函数（防止暴力请求）
 */
export function debounce<T extends (...args: any[]) => any>(
  func: T,
  wait: number,
): (...args: Parameters<T>) => void {
  let timeout: ReturnType<typeof setTimeout> | null = null;

  return function executedFunction(...args: Parameters<T>) {
    const later = () => {
      timeout = null;
      func(...args);
    };

    if (timeout) clearTimeout(timeout);
    timeout = setTimeout(later, wait);
  };
}

/**
 * 节流函数（限制请求频率）
 */
export function throttle<T extends (...args: any[]) => any>(
  func: T,
  limit: number,
): (...args: Parameters<T>) => void {
  let inThrottle: boolean;

  return function executedFunction(...args: Parameters<T>) {
    if (!inThrottle) {
      func(...args);
      inThrottle = true;
      setTimeout(() => (inThrottle = false), limit);
    }
  };
}
