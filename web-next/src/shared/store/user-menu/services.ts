import type { RealnameStatusResponse, UserDetailResponse } from "./types";

import { getUserProfile } from "@/lib/api/generated/sdk.gen";
import { apiClient } from "@/lib/api";
import { getSessionContext } from "@/shared/session";
import { request } from "@/shared/http";

export const getUserDetail = async (): Promise<UserDetailResponse> => {
  const { data } = await getUserProfile({ client: apiClient });

  return {
    principal_id: String(data.id),
    principal_type: data.role,
    display_name: data.username,
    phone: "",
    email: data.email,
    phone_verified: false,
    email_verified: Boolean(data.email),
  };
};

export const getRealnameStatus = async (): Promise<RealnameStatusResponse> => {
  const sessionContext = await getSessionContext();
  const realname = sessionContext.realname;

  if (!realname?.applicable || !realname.has_record) {
    return {
      has_verification: false,
    };
  }

  const type =
    realname.type_code === "enterprise" ? "enterprise" : "personal";
  const status =
    realname.status_code === "verified"
      ? "approved"
      : realname.status_code === "recheck"
        ? "verifying"
        : realname.status_code === "abnormal"
          ? "rejected"
          : "pending";

  return {
    has_verification: true,
    verification: {
      id: "",
      user_id: sessionContext.principal.principal_id,
      type,
      status,
      real_name: "",
      company_name: undefined,
    },
  };
};

export const logout = async (): Promise<void> => {
  await request.post("/auth/logout");
};
