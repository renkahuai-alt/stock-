import { writable } from 'svelte/store';

export interface ChartViewportState {
  autoFollowLatest: boolean;
}

export const chartViewportStore = writable<ChartViewportState>({
  autoFollowLatest: true,
});

export function setChartViewportAutoFollowLatest(autoFollowLatest: boolean): void {
  chartViewportStore.update((current) => (
    current.autoFollowLatest === autoFollowLatest
      ? current
      : { autoFollowLatest }
  ));
}
