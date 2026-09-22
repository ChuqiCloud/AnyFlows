export function resolveFeatureState<TState>(
  state: TState | undefined,
  initialState: TState,
): TState {
  return state ?? initialState;
}
