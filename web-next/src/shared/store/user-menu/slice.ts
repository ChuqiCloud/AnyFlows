import type { UserMenuInfo, UserMenuRealnameStatus } from "./types";

import { createAsyncThunk, createSlice } from "@reduxjs/toolkit";

import { tokenManager } from "@/shared/auth/kernel";

import { getRealnameStatus, getUserDetail, logout } from "./services";

interface UserMenuState {
  userInfo: UserMenuInfo | null;
  realnameStatus: UserMenuRealnameStatus | null;
  loading: {
    userInfo: boolean;
    realnameStatus: boolean;
  };
  error: {
    userInfo: string | null;
    realnameStatus: string | null;
  };
}

const initialState: UserMenuState = {
  userInfo: null,
  realnameStatus: null,
  loading: {
    userInfo: false,
    realnameStatus: false,
  },
  error: {
    userInfo: null,
    realnameStatus: null,
  },
};

export const fetchUserMenuInfo = createAsyncThunk(
  "userMenu/fetchUserInfo",
  async (_, { rejectWithValue }) => {
    try {
      const data = await getUserDetail();

      return {
        principalId: data.principal_id,
        displayName: data.display_name,
        principalType: data.principal_type,
        phone: data.phone,
        email: data.email,
        phoneVerified: data.phone_verified,
        emailVerified: data.email_verified,
      } as UserMenuInfo;
    } catch (err: any) {
      return rejectWithValue(err.message || "获取用户信息失败");
    }
  },
  {
    condition: (_, { getState }) => {
      const state = getState() as { userMenu: UserMenuState };

      return !state.userMenu.loading.userInfo && !state.userMenu.userInfo;
    },
  },
);

export const logoutUser = createAsyncThunk(
  "userMenu/logout",
  async (_, { rejectWithValue }) => {
    try {
      await logout();
      tokenManager.clearTokens();
    } catch (err: any) {
      tokenManager.clearTokens();

      return rejectWithValue(err.message || "登出失败");
    }
  },
);

export const fetchUserMenuRealnameStatus = createAsyncThunk(
  "userMenu/fetchRealnameStatus",
  async () => {
    try {
      const data = await getRealnameStatus();
      const verification = data.verification;

      return {
        type:
          verification?.type === "personal"
            ? "individual"
            : verification?.type || "",
        status: verification?.status || "none",
        realName: verification?.real_name,
        companyName: verification?.company_name,
      } as UserMenuRealnameStatus;
    } catch {
      return {
        type: "",
        status: "none",
      } as UserMenuRealnameStatus;
    }
  },
  {
    condition: (_, { getState }) => {
      const state = getState() as { userMenu: UserMenuState };

      return (
        !state.userMenu.loading.realnameStatus &&
        !state.userMenu.realnameStatus
      );
    },
  },
);

const userMenuSlice = createSlice({
  name: "userMenu",
  initialState,
  reducers: {
    clearUserMenu: () => initialState,
  },
  extraReducers: (builder) => {
    builder
      .addCase(fetchUserMenuInfo.pending, (state) => {
        state.loading.userInfo = true;
        state.error.userInfo = null;
      })
      .addCase(fetchUserMenuInfo.fulfilled, (state, action) => {
        state.loading.userInfo = false;
        state.userInfo = action.payload;
      })
      .addCase(fetchUserMenuInfo.rejected, (state, action) => {
        state.loading.userInfo = false;
        state.error.userInfo = action.payload as string;
      })
      .addCase(fetchUserMenuRealnameStatus.pending, (state) => {
        state.loading.realnameStatus = true;
        state.error.realnameStatus = null;
      })
      .addCase(fetchUserMenuRealnameStatus.fulfilled, (state, action) => {
        state.loading.realnameStatus = false;
        state.realnameStatus = action.payload;
      })
      .addCase(fetchUserMenuRealnameStatus.rejected, (state, action) => {
        state.loading.realnameStatus = false;
        state.error.realnameStatus = action.payload as string;
      });
  },
});

export const { clearUserMenu } = userMenuSlice.actions;
export const userMenuReducer = userMenuSlice.reducer;

export const selectUserMenuInfo = (state: { userMenu: UserMenuState }) =>
  state.userMenu.userInfo;

export const selectUserMenuRealnameStatus = (state: {
  userMenu: UserMenuState;
}) => state.userMenu.realnameStatus;

export const selectUserMenuLoading = (state: { userMenu: UserMenuState }) =>
  state.userMenu.loading.userInfo || state.userMenu.loading.realnameStatus;
