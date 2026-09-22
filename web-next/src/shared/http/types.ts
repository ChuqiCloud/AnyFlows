import type { AxiosError, AxiosInstance, AxiosRequestConfig } from "axios";
import type { BaseResponse, ErrorResponse } from "@/shared/session/contracts";

export interface RequestContext {
  accessToken?: string | null;
  tenantId?: string | null;
  requestId?: string | null;
}

export interface HttpErrorShape {
  code: string;
  message: string;
  status?: number;
  requestId?: string;
  details?: Record<string, unknown>;
}

export interface RequestOptions extends AxiosRequestConfig {
  context?: RequestContext;
}

export interface HttpClient {
  get: <T = unknown>(
    url: string,
    config?: AxiosRequestConfig,
  ) => Promise<BaseResponse<T>>;
  post: <T = unknown>(
    url: string,
    body?: unknown,
    config?: AxiosRequestConfig,
  ) => Promise<BaseResponse<T>>;
  put: <T = unknown>(
    url: string,
    body?: unknown,
    config?: AxiosRequestConfig,
  ) => Promise<BaseResponse<T>>;
  delete: <T = unknown>(
    url: string,
    config?: AxiosRequestConfig,
  ) => Promise<BaseResponse<T>>;
  patch: <T = unknown>(
    url: string,
    body?: unknown,
    config?: AxiosRequestConfig,
  ) => Promise<BaseResponse<T>>;
}

export interface HttpInterceptorRegistration {
  eject: () => void;
}

export type HttpAxiosError = AxiosError<ErrorResponse>;
export type HttpAxiosInstance = AxiosInstance;
