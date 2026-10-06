const normalizeEnvValue = (value: unknown) =>
  typeof value === "string" ? value.trim() : "";

/**
 * 后端基址：开发环境固定为空串，由 Vite 同源代理转发到 `VITE_API_BASE_URL` 指定的后端；
 * 生产环境默认同源（后端内嵌前端），仅独立部署时才由 `VITE_API_BASE_URL` 覆盖。
 */
export const resolveApiBaseUrl = () =>
  import.meta.env.DEV
    ? ""
    : normalizeEnvValue(import.meta.env.VITE_API_BASE_URL).replace(/\/+$/, "");

/** 拼接后端请求地址；基址为空时返回同源相对路径。 */
export const apiUrl = (path: string) => `${resolveApiBaseUrl()}${path}`;
