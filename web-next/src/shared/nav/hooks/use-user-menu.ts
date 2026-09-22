import type { AppDispatch } from "@/shared/store";

import { useCallback, useEffect } from "react";
import { useDispatch, useSelector } from "react-redux";

import {
  getCurrentReturnPath,
  redirectToAuth,
} from "@/shared/auth/kernel";
import {
  clearPlatformState,
  useSession,
} from "@/shared/session";
import { selectSessionPrincipal } from "@/shared/store/session";
import {
  clearUserMenu,
  fetchUserMenuInfo,
  fetchUserMenuRealnameStatus,
  logoutUser,
  selectUserMenuInfo,
  selectUserMenuLoading,
  selectUserMenuRealnameStatus,
} from "@/shared/store/user-menu";
export const useUserMenu = () => {
  const dispatch = useDispatch<AppDispatch>();
  const session = useSession();
  const principalId = useSelector(selectSessionPrincipal)?.principal_id ?? null;

  const userInfo = useSelector(selectUserMenuInfo);
  const realnameStatus = useSelector(selectUserMenuRealnameStatus);
  const loading = useSelector(selectUserMenuLoading);

  useEffect(() => {
    if (principalId) {
      dispatch(fetchUserMenuInfo());
      dispatch(fetchUserMenuRealnameStatus());
    }
  }, [dispatch, principalId]);

  const isVerified = realnameStatus?.status === "approved";
  const isPending =
    realnameStatus?.status === "pending" ||
    realnameStatus?.status === "verifying";

  const handleLogout = useCallback(async () => {
    try {
      await dispatch(logoutUser()).unwrap();
    } catch {
      // ignore remote logout failures; local state still needs to clear
    }

    clearPlatformState(dispatch, { resetAccess: true });
    dispatch(clearUserMenu());
    void redirectToAuth(getCurrentReturnPath());
  }, [dispatch]);

  return {
    userInfo,
    realnameStatus,
    loading,
    isVerified,
    isPending,
    contextStatus: session.contextStatus,
    contextError: session.contextError,
    retryContextSync: session.retryContextSync,
    handleLogout,
  };
};
