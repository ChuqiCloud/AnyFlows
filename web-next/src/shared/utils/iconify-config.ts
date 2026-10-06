/***
 * Iconify 自建 API 配置
 * 多项目共用，支持缓存
 */
import { addAPIProvider } from "@iconify/react";

// 自建 Iconify API 地址
const ICONIFY_API_URL = "https://icon-api.cdn.moeact.net";

/**
 * 初始化 Iconify 配置
 */
export const initIconify = () => {
  // 配置自建 API
  addAPIProvider("", {
    resources: [ICONIFY_API_URL],
  });
};
