import type { PayloadAction } from "@reduxjs/toolkit";
import type { RootState } from "@/shared/store";
import type {
  OperationChallenge,
  OperationGrant,
  OperationSnapshot,
} from "@/shared/operation/types";

import { createSlice } from "@reduxjs/toolkit";

const initialState: OperationSnapshot = {
  currentChallenge: null,
  currentGrant: null,
  isReady: false,
};

const operationSlice = createSlice({
  name: "operation",
  initialState,
  reducers: {
    hydrateOperationSnapshot: (
      state,
      action: PayloadAction<Partial<OperationSnapshot>>,
    ) => ({
      ...state,
      ...action.payload,
    }),
    setOperationSnapshot: (
      state,
      action: PayloadAction<Partial<OperationSnapshot>>,
    ) => ({
      ...state,
      ...action.payload,
      isReady: action.payload.isReady ?? true,
    }),
    setOperationChallenge: (state, action: PayloadAction<OperationChallenge>) => {
      state.currentChallenge = action.payload;
      state.currentGrant = action.payload.grantToken
        ? {
            token: action.payload.grantToken,
            operationType: action.payload.operationType,
            expiresAt:
              action.payload.grantExpiresAt ?? action.payload.expiresAt ?? null,
          }
        : null;
      state.isReady = true;
    },
    patchOperationChallenge: (
      state,
      action: PayloadAction<Partial<OperationChallenge>>,
    ) => {
      if (!state.currentChallenge) {
        return;
      }

      state.currentChallenge = {
        ...state.currentChallenge,
        ...action.payload,
      };

      if (action.payload.grantToken) {
        state.currentGrant = {
          token: action.payload.grantToken,
          operationType: state.currentChallenge.operationType,
          expiresAt:
            state.currentChallenge.grantExpiresAt ??
            state.currentChallenge.expiresAt ??
            null,
        };
      }

      state.isReady = true;
    },
    setOperationGrant: (state, action: PayloadAction<OperationGrant | null>) => {
      state.currentGrant = action.payload;
      state.isReady = true;
    },
    clearOperationChallenge: (state) => {
      state.currentChallenge = null;
      state.isReady = true;
    },
    clearOperationGrant: (state) => {
      state.currentGrant = null;
      state.isReady = true;
    },
    resetOperation: () => initialState,
  },
});

export const {
  hydrateOperationSnapshot,
  setOperationSnapshot,
  setOperationChallenge,
  patchOperationChallenge,
  setOperationGrant,
  clearOperationChallenge,
  clearOperationGrant,
  resetOperation,
} = operationSlice.actions;

export const operationReducer = operationSlice.reducer;

export const selectOperationState = (state: RootState) => state.operation;
export const selectCurrentOperationChallenge = (state: RootState) =>
  state.operation.currentChallenge;
export const selectCurrentOperationGrant = (state: RootState) =>
  state.operation.currentGrant;
export const selectOperationIsReady = (state: RootState) =>
  state.operation.isReady;
