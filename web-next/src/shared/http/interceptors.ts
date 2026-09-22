import { AxiosHeaders } from "axios";

import { invalidateManagementSession } from "@/lib/api/session-token";
import {
  dispatchAuthInvalidatedEvent,
  dispatchAuthStateChangedEvent,
  tokenManager,
} from "@/shared/auth/kernel";
import type { ErrorResponse } from "@/shared/session/contracts";
import { tenantStorage } from "@/shared/tenant";
import { throttle } from "@/shared/utils/security";

import type {
  HttpAxiosError,
  HttpAxiosInstance,
  HttpInterceptorRegistration,
} from "./types";

let requestCount = 0;
let requestResetTimer: ReturnType<typeof setTimeout> | null = null;

const ensureHeaders = (headers?: unknown) => {
  if (headers instanceof AxiosHeaders) {
    return headers;
  }

  return AxiosHeaders.from((headers ?? undefined) as any);
};

const resetRequestCount = throttle(() => {
  requestCount = 0;
}, 1000);

/** 登录请求自身的 401 是凭据错误，不能当成会话失效处理。 */
const isLoginRequest = (url?: string) => Boolean(url?.includes("/auth/login"));

export function registerHttpInterceptors(
  axiosInstance: HttpAxiosInstance,
): HttpInterceptorRegistration {
  const requestInterceptorId = axiosInstance.interceptors.request.use(
    (config) => {
      requestCount++;
      if (requestCount > 50) {
        window.dispatchEvent(new CustomEvent("auth:request-frequency-high"));
      }

      if (requestResetTimer) {
        clearTimeout(requestResetTimer);
      }

      requestResetTimer = setTimeout(() => {
        requestCount = 0;
      }, 1000);

      const headers = ensureHeaders(config.headers);
      const token = tokenManager.getAccessToken();

      if (token) {
        headers.set("Authorization", `Bearer ${token}`);
      }

      const currentTenantId = tenantStorage.getCurrentTenantId();

      if (currentTenantId) {
        headers.set("X-Tenant-ID", currentTenantId);
        headers.set("X-Moe-Tenant-ID", currentTenantId);
      }

      headers.set("X-Request-Time", Date.now().toString());
      headers.set(
        "X-Request-ID",
        `${Date.now()}-${Math.random().toString(36).slice(2, 11)}`,
      );
      config.headers = headers;

      return config;
    },
    (error) => Promise.reject(error),
  );

  const responseInterceptorId = axiosInstance.interceptors.response.use(
    (response) => {
      resetRequestCount();

      return response.data;
    },
    (error: HttpAxiosError) => {
      if (!error.response) {
        return Promise.reject(error);
      }

      const { status, data } = error.response;
      const url = error.config?.url;

      // AnyFlows 没有刷新令牌：401 直接判定会话失效，清理令牌并通知登录边界。
      if (status === 401 && !isLoginRequest(url)) {
        if (invalidateManagementSession()) {
          dispatchAuthStateChangedEvent({ kind: "anonymous" });
          dispatchAuthInvalidatedEvent({
            reason: "unauthorized",
            source: "http",
          });
        }
      }

      if (status === 403) {
        window.dispatchEvent(
          new CustomEvent("auth:forbidden", {
            detail: { url },
          }),
        );
      }

      if (status === 429) {
        window.dispatchEvent(new CustomEvent("auth:rate-limit"));
      }

      return Promise.reject((data as ErrorResponse) || error);
    },
  );

  return {
    eject() {
      axiosInstance.interceptors.request.eject(requestInterceptorId);
      axiosInstance.interceptors.response.eject(responseInterceptorId);
    },
  };
}
