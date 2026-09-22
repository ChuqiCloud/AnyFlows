import type { AppDispatch, RootState } from "./index";

import {
  type TypedUseSelectorHook,
  useDispatch,
  useSelector,
} from "react-redux";

// 使用类型化的 hooks，避免每次使用时都要指定类型
export const useAppDispatch = () => useDispatch<AppDispatch>();
export const useAppSelector: TypedUseSelectorHook<RootState> = useSelector;
