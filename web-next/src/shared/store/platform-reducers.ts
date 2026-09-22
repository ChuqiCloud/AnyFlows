import { accessControlReducer } from "./access-control";
import { operationReducer } from "./operation";
import { sessionReducer } from "./session";
import { userMenuReducer } from "./user-menu";

export const platformReducers = {
  session: sessionReducer,
  accessControl: accessControlReducer,
  operation: operationReducer,
  userMenu: userMenuReducer,
};
