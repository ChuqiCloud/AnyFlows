import type { AnyAction, Reducer, ReducersMapObject } from "@reduxjs/toolkit";

import { combineReducers } from "@reduxjs/toolkit";

export interface ReducerManager<TState, TReducerMap extends ReducersMapObject> {
  reduce: Reducer<TState, AnyAction>;
  add: <K extends keyof TReducerMap>(key: K, reducer: TReducerMap[K]) => boolean;
  remove: <K extends keyof TReducerMap>(key: K) => boolean;
  has: <K extends keyof TReducerMap>(key: K) => boolean;
  getReducerMap: () => Partial<TReducerMap>;
}

export function createReducerManager<TState, TReducerMap extends ReducersMapObject>(
  initialReducers: Partial<TReducerMap>,
): ReducerManager<TState, TReducerMap> {
  const reducers: Partial<TReducerMap> = { ...initialReducers };
  let combinedReducer = combineReducers(
    reducers as unknown as ReducersMapObject,
  ) as Reducer<TState, AnyAction>;
  let keysToRemove: (keyof TReducerMap)[] = [];

  return {
    reduce: (state, action) => {
      if (keysToRemove.length > 0 && state && typeof state === "object") {
        const nextState = { ...(state as Record<string, unknown>) };

        for (const key of keysToRemove) {
          delete nextState[key as string];
        }

        keysToRemove = [];

        return combinedReducer(nextState as TState, action);
      }

      return combinedReducer(state, action);
    },
    add: (key, reducer) => {
      if (!key || reducers[key] === reducer) {
        return false;
      }

      reducers[key] = reducer;
      combinedReducer = combineReducers(
        reducers as unknown as ReducersMapObject,
      ) as Reducer<TState, AnyAction>;

      return true;
    },
    remove: (key) => {
      if (!key || !reducers[key]) {
        return false;
      }

      delete reducers[key];
      keysToRemove.push(key);
      combinedReducer = combineReducers(
        reducers as unknown as ReducersMapObject,
      ) as Reducer<TState, AnyAction>;

      return true;
    },
    has: (key) => Boolean(reducers[key]),
    getReducerMap: () => ({ ...reducers }),
  };
}
