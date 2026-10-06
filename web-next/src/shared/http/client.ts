import type { AxiosRequestConfig } from "axios";
import type { BaseResponse } from "@/shared/session/contracts";
import type { HttpClient } from "./types";

import axios from "axios";

import { resolveApiBaseUrl } from "@/shared/utils/env";

import { registerHttpInterceptors } from "./interceptors";

const axiosInstance = axios.create({
  baseURL: resolveApiBaseUrl(),
  timeout: 30000,
  headers: {
    "Content-Type": "application/json",
    "X-Requested-With": "XMLHttpRequest",
  },
  withCredentials: true,
});

registerHttpInterceptors(axiosInstance);

export const request: HttpClient = {
  get: <T = unknown>(url: string, config?: AxiosRequestConfig) =>
    axiosInstance.get<unknown, BaseResponse<T>>(url, config),

  post: <T = unknown>(
    url: string,
    data?: unknown,
    config?: AxiosRequestConfig,
  ) => axiosInstance.post<unknown, BaseResponse<T>>(url, data, config),

  put: <T = unknown>(
    url: string,
    data?: unknown,
    config?: AxiosRequestConfig,
  ) => axiosInstance.put<unknown, BaseResponse<T>>(url, data, config),

  delete: <T = unknown>(url: string, config?: AxiosRequestConfig) =>
    axiosInstance.delete<unknown, BaseResponse<T>>(url, config),

  patch: <T = unknown>(
    url: string,
    data?: unknown,
    config?: AxiosRequestConfig,
  ) => axiosInstance.patch<unknown, BaseResponse<T>>(url, data, config),
};

export { axiosInstance };
export default axiosInstance;
