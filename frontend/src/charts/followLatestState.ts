export const FOLLOW_LATEST_SCROLL_TOLERANCE_BARS = 0.5;

export interface FollowLatestState {
  viewportKey: string;
  autoFollowLatest: boolean;
  anchorScrollPosition: number | null;
}

export interface FollowLatestRangeChangeInput {
  scrollPosition: number;
  isProgrammaticChange: boolean;
  toleranceBars?: number;
}

export function createFollowLatestState(): FollowLatestState {
  return {
    viewportKey: '',
    autoFollowLatest: true,
    anchorScrollPosition: null,
  };
}

export function resolveFollowLatestStateOnViewportChange(
  current: FollowLatestState,
  viewportKey: string,
): FollowLatestState {
  if (current.viewportKey === viewportKey) {
    return current;
  }

  return {
    viewportKey,
    autoFollowLatest: true,
    anchorScrollPosition: null,
  };
}

export function resolveFollowLatestStateOnRangeChange(
  current: FollowLatestState,
  input: FollowLatestRangeChangeInput,
): FollowLatestState {
  if (input.isProgrammaticChange || current.anchorScrollPosition === null) {
    return current;
  }

  const toleranceBars = input.toleranceBars ?? FOLLOW_LATEST_SCROLL_TOLERANCE_BARS;
  const autoFollowLatest = Math.abs(input.scrollPosition - current.anchorScrollPosition) <= toleranceBars;

  if (autoFollowLatest === current.autoFollowLatest) {
    return current;
  }

  return {
    ...current,
    autoFollowLatest,
  };
}

export function withAnchorScrollPosition(
  current: FollowLatestState,
  anchorScrollPosition: number | null,
): FollowLatestState {
  if (current.anchorScrollPosition === anchorScrollPosition) {
    return current;
  }

  return {
    ...current,
    anchorScrollPosition,
  };
}
