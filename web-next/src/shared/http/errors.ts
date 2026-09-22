import type { AxiosError } from "axios";
import type { ErrorResponse } from "@/shared/session/contracts";
import type { HttpErrorShape } from "./types";

export function normalizeHttpError(error: unknown): HttpErrorShape {
  const axiosError = error as AxiosError<ErrorResponse>;

  if (axiosError?.response?.data?.error) {
    return {
      code: axiosError.response.data.error.code,
      message: axiosError.response.data.error.message,
      status: axiosError.response.status,
      requestId: axiosError.response.data.request_id,
      details: axiosError.response.data.error.details,
    };
  }

  if (error instanceof Error) {
    return {
      code: "UNKNOWN_ERROR",
      message: error.message,
    };
  }

  return {
    code: "UNKNOWN_ERROR",
    message: "Unknown request error",
  };
}
