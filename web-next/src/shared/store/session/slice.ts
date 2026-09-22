import type { PayloadAction } from "@reduxjs/toolkit";
import type { RootState } from "@/shared/store";
import type {
  SessionBootstrapPayload,
  SessionState,
} from "@/shared/session/types";
import type { PrincipalProfile } from "@/shared/session/contracts";

import { createSlice } from "@reduxjs/toolkit";

import {
  deriveAuthState,
  isAuthAuthenticated,
  isAuthReady,
  tokenManager,
  type AuthState,
} from "@/shared/auth/kernel";

import { patchSessionPrincipal } from "./actions";

const initialState: SessionState = {
  authState: { kind: "restoring" },
  principal: null,
  realname: null,
  isAuthenticated: false,
  isReady: false,
  contextStatus: "idle",
  contextError: null,
};

const sessionSlice = createSlice({
  name: "session",
  initialState,
  reducers: {
    hydrateSessionState: (state, action: PayloadAction<Partial<SessionState>>) => {
      const next = {
        ...state,
        ...action.payload,
      };

      state.principal = next.principal;
      state.realname = action.payload.realname ?? next.realname ?? null;
      state.isAuthenticated =
        action.payload.isAuthenticated ?? !!next.principal;
      state.isReady = action.payload.isReady ?? next.isReady;
      state.authState =
        action.payload.authState ??
        deriveAuthState({
          principal: next.principal,
          accessToken: tokenManager.getAccessToken(),
          isAuthenticated: state.isAuthenticated,
          isReady: state.isReady,
          previous: next.authState,
        });
      state.contextStatus = action.payload.contextStatus ?? next.contextStatus;
      state.contextError = action.payload.contextError ?? next.contextError;
    },
    bootstrapSession: (
      state,
      action: PayloadAction<SessionBootstrapPayload>,
    ) => {
      state.principal = action.payload.principal;
      state.realname = action.payload.realname ?? null;
      state.isAuthenticated =
        action.payload.isAuthenticated ?? !!action.payload.principal;
      state.isReady = true;
      state.authState =
        action.payload.authState ??
        deriveAuthState({
          principal: state.principal,
          accessToken: tokenManager.getAccessToken(),
          isAuthenticated: state.isAuthenticated,
          isReady: state.isReady,
          previous: state.authState,
        });
      if (action.payload.principal) {
        state.contextStatus = "ready";
        state.contextError = null;
      }
    },
    setSessionContextState: (
      state,
      action: PayloadAction<{
        status: SessionState["contextStatus"];
        error?: string | null;
      }>,
    ) => {
      state.contextStatus = action.payload.status;
      state.contextError = action.payload.error ?? null;
    },
    setSessionAuthState: (state, action: PayloadAction<AuthState>) => {
      state.authState = action.payload;
      state.isReady = isAuthReady(action.payload);
      state.isAuthenticated = isAuthAuthenticated(action.payload);
    },
    clearSession: () => ({
      ...initialState,
      authState: { kind: "anonymous" as const },
      isReady: true,
    }),
  },
  extraReducers: (builder) => {
    builder.addCase(patchSessionPrincipal, (state, action) => {
      if (!state.principal) {
        return;
      }

      state.principal = {
        ...state.principal,
        ...action.payload,
      } as PrincipalProfile;
      tokenManager.setPrincipal(state.principal);
    });
  },
});

export const {
  hydrateSessionState,
  bootstrapSession,
  setSessionContextState,
  setSessionAuthState,
  clearSession,
} = sessionSlice.actions;

export const sessionReducer = sessionSlice.reducer;

export const selectSessionState = (state: RootState) => state.session;
export const selectSessionAuthState = (state: RootState) => state.session.authState;
export const selectSessionPrincipal = (state: RootState) =>
  state.session.principal;
export const selectSessionRealname = (state: RootState) => state.session.realname;
export const selectSessionIsAuthenticated = (state: RootState) =>
  state.session.isAuthenticated;
export const selectSessionIsReady = (state: RootState) => state.session.isReady;
export const selectSessionContextStatus = (state: RootState) =>
  state.session.contextStatus;
export const selectSessionContextError = (state: RootState) =>
  state.session.contextError;
