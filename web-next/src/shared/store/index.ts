import type { AnyAction, EnhancedStore, ReducersMapObject } from "@reduxjs/toolkit";
import type { FeatureStateRegistry, FeatureReducersMap } from "./feature-registry";

import { configureStore } from "@reduxjs/toolkit";

import { platformReducers } from "./platform-reducers";
import { createReducerManager } from "./reducer-manager";

export type PlatformState = {
  [K in keyof typeof platformReducers]: ReturnType<(typeof platformReducers)[K]>;
};

export type RootStateShape = PlatformState & Partial<FeatureStateRegistry>;

type RootReducersMap = ReducersMapObject<RootStateShape, AnyAction>;

const staticReducers = {
  ...platformReducers,
} as Partial<RootReducersMap>;

const reducerManager = createReducerManager<RootStateShape, RootReducersMap>(
  staticReducers,
);

// 业务分片不在此静态聚合：它由 injectReducers 在模块路由懒加载时注入，
// 静态引用会让 store 反向依赖 @/feature/modules 形成初始化环（TDZ）。
export const store = configureStore({
  reducer: reducerManager.reduce,
});

type AppStore = EnhancedStore<RootStateShape, AnyAction> & {
  reducerManager: typeof reducerManager;
  injectReducers: (reducers: Partial<FeatureReducersMap>) => boolean;
};

const enhancedStore = store as AppStore;

enhancedStore.reducerManager = reducerManager;
enhancedStore.injectReducers = (reducers) => {
  let changed = false;

  for (const key of Object.keys(reducers) as (keyof FeatureReducersMap)[]) {
    const reducer = reducers[key];

    if (!reducer) {
      continue;
    }

    changed = reducerManager.add(key, reducer as RootReducersMap[typeof key]) || changed;
  }

  if (changed) {
    enhancedStore.dispatch({
      type: "@@store/INJECT_FEATURE_REDUCERS",
    });
  }

  return changed;
};

enhancedStore.dispatch({
  type: "@@store/INIT_FEATURE_REDUCERS",
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;
export type { AppStore };
