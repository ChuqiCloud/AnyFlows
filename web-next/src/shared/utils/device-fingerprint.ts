/**
 * 设备指纹工具
 * 使用 FingerprintJS 收集客户端设备信息用于风控和设备识别
 *
 * 安装依赖：
 * pnpm add @fingerprintjs/fingerprintjs
 */

import FingerprintJS from "@fingerprintjs/fingerprintjs";

export interface DeviceData {
  screen_width: number;
  screen_height: number;
  timezone: string;
  language: string;
  fingerprint?: string; // FingerprintJS 生成的唯一指纹
}

let fpPromise: Promise<any> | null = null;

/**
 * 初始化 FingerprintJS（懒加载）
 */
function initFingerprint() {
  if (!fpPromise) {
    fpPromise = FingerprintJS.load();
  }

  return fpPromise;
}

/**
 * 收集设备数据（包含 FingerprintJS 指纹）
 * @returns 设备数据对象（包含唯一指纹）
 */
export async function collectDeviceData(): Promise<DeviceData> {
  const basicData = {
    screen_width: window.screen.width,
    screen_height: window.screen.height,
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    language: navigator.language || navigator.languages?.[0] || "zh-CN",
  };

  try {
    const fp = await initFingerprint();
    const result = await fp.get();

    return {
      ...basicData,
      fingerprint: result.visitorId, // 唯一的设备指纹 ID
    };
  } catch (error) {
    console.warn("生成设备指纹失败，已退回到基础设备信息：", error);

    return basicData;
  }
}

/**
 * 获取设备描述（用于展示）
 */
export function getDeviceDescription(): string {
  const ua = navigator.userAgent;
  const platform = navigator.platform;

  // 简单的设备类型判断
  const isMobile = /Mobile|Android|iPhone|iPad|iPod/i.test(ua);
  const isTablet = /iPad|Android.*Tablet/i.test(ua);

  let deviceType = "桌面设备";

  if (isTablet) {
    deviceType = "平板设备";
  } else if (isMobile) {
    deviceType = "移动设备";
  }

  return `${deviceType} (${platform})`;
}

/**
 * 获取浏览器指纹 ID（仅指纹，不包含其他设备数据）
 * 用于快速获取设备唯一标识
 */
export async function getFingerprint(): Promise<string> {
  try {
    const fp = await initFingerprint();
    const result = await fp.get();

    return result.visitorId;
  } catch (error) {
    console.error("获取设备指纹失败：", error);
    // 降级方案：使用简单的哈希
    const fallback = `${navigator.userAgent}-${window.screen.width}x${window.screen.height}`;

    return btoa(fallback).substring(0, 32);
  }
}
