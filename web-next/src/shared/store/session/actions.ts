import type { PrincipalProfile } from "@/shared/session";

import { createAction } from "@reduxjs/toolkit";

export const patchSessionPrincipal = createAction<Partial<PrincipalProfile>>(
  "session/patchPrincipal",
);
