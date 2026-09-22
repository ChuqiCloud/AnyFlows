import type { PayloadAction } from "@reduxjs/toolkit";
import type { RootState } from "@/shared/store";
import type { AccessSnapshot } from "@/shared/access-control/types";

import { createSlice } from "@reduxjs/toolkit";

const initialState: AccessSnapshot = {
  capabilities: [],
  surface: "user",
  status: "idle",
  error: null,
};

const accessControlSlice = createSlice({
  name: "accessControl",
  initialState,
  reducers: {
    setAccessSnapshot: (
      state,
      action: PayloadAction<Partial<AccessSnapshot>>,
    ) => {
      if (action.payload.capabilities !== undefined) {
        state.capabilities = action.payload.capabilities;
      }

      if (action.payload.surface !== undefined) {
        state.surface = action.payload.surface;
      }

      if (action.payload.status !== undefined) {
        state.status = action.payload.status;
      }

      if (action.payload.error !== undefined) {
        state.error = action.payload.error;
      }
    },
    resetAccessControl: () => initialState,
  },
});

export const {
  resetAccessControl,
  setAccessSnapshot,
} = accessControlSlice.actions;

export const accessControlReducer = accessControlSlice.reducer;

export const selectAccessControlState = (state: RootState) =>
  state.accessControl;
