<script lang="ts">
  import { onMount } from 'svelte';
  import type { ChartViewStatus } from '../stores/chartStore';
  import type { ActiveOverlayPayload, BarPoint } from '../types/contracts';
  import { lightweightChartController } from '../charts/lightweightController';

  export let bars: BarPoint[] = [];
  export let activeOverlay: ActiveOverlayPayload | undefined;
  export let status: ChartViewStatus = 'ready';
  export let headline = 'Lightweight Charts 单实例图表';
  export let detail = '支持指数、板块与个股的统一展示';
  export let viewportKey = '';
  export let initialVisibleBars = 132;

  const WHEEL_AXIS_DOMINANCE_RATIO = 1.25;
  const WHEEL_GESTURE_LOCK_MS = 140;

  const controller = lightweightChartController;
  let chartHostEl: HTMLDivElement;

  $: showOverlay = status !== 'ready' || bars.length === 0;

  onMount(() => {
    let layoutRefreshFrame = 0;
    let layoutRefreshTimeout: ReturnType<typeof window.setTimeout> | null = null;
    let wheelGestureResetTimeout: ReturnType<typeof window.setTimeout> | null = null;
    let activeWheelIntent: 'zoom' | 'pan' | null = null;
    const scheduleLayoutRefresh = () => {
      if (typeof window === 'undefined') {
        return;
      }

      if (layoutRefreshFrame) {
        window.cancelAnimationFrame(layoutRefreshFrame);
      }

      if (layoutRefreshTimeout !== null) {
        window.clearTimeout(layoutRefreshTimeout);
      }

      layoutRefreshFrame = window.requestAnimationFrame(() => {
        controller.refreshLayout();
      });

      layoutRefreshTimeout = window.setTimeout(() => {
        controller.refreshLayout();
        layoutRefreshTimeout = null;
      }, 120);
    };
    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible') {
        scheduleLayoutRefresh();
      }
    };
    const clearWheelGestureReset = () => {
      if (wheelGestureResetTimeout !== null) {
        window.clearTimeout(wheelGestureResetTimeout);
        wheelGestureResetTimeout = null;
      }
    };
    const scheduleWheelGestureReset = () => {
      if (typeof window === 'undefined') {
        return;
      }

      clearWheelGestureReset();
      wheelGestureResetTimeout = window.setTimeout(() => {
        activeWheelIntent = null;
        wheelGestureResetTimeout = null;
      }, WHEEL_GESTURE_LOCK_MS);
    };
    const resolveWheelIntent = (event: WheelEvent): 'zoom' | 'pan' | null => {
      if (event.ctrlKey) {
        activeWheelIntent = 'zoom';
        return activeWheelIntent;
      }

      if (activeWheelIntent) {
        return activeWheelIntent;
      }

      const absDeltaX = Math.abs(event.deltaX);
      const absDeltaY = Math.abs(event.deltaY);
      const isHorizontalDominant = Math.abs(event.deltaX) > Math.abs(event.deltaY);
      const isVerticalDominant = Math.abs(event.deltaY) > Math.abs(event.deltaX);
      const isMostlyHorizontal = Math.abs(event.deltaX) > Math.abs(event.deltaY) * WHEEL_AXIS_DOMINANCE_RATIO;
      const isMostlyVertical = Math.abs(event.deltaY) > Math.abs(event.deltaX) * WHEEL_AXIS_DOMINANCE_RATIO;

      if (isMostlyHorizontal || (isHorizontalDominant && absDeltaY < 0.5)) {
        activeWheelIntent = 'pan';
        return activeWheelIntent;
      }

      if (isMostlyVertical || (isVerticalDominant && absDeltaX < 0.5)) {
        activeWheelIntent = 'zoom';
        return activeWheelIntent;
      }

      return null;
    };
    const handleWheel = (event: WheelEvent) => {
      event.preventDefault();

      const wheelIntent = resolveWheelIntent(event);

      if (wheelIntent === 'pan') {
        controller.applyHorizontalPan(event.deltaX);
        scheduleWheelGestureReset();
        return;
      }

      if (wheelIntent === 'zoom') {
        controller.applyProportionalZoom(event.deltaY, event.clientX);
        scheduleWheelGestureReset();
      }
    };

    controller.setViewportKey(viewportKey);
    controller.setInitialVisibleBars(initialVisibleBars);
    controller.mount(chartHostEl);
    controller.setData(bars);

    scheduleLayoutRefresh();
    chartHostEl.addEventListener('wheel', handleWheel, { passive: false, capture: true });
    document.addEventListener('visibilitychange', handleVisibilityChange);
    window.addEventListener('focus', scheduleLayoutRefresh);
    window.addEventListener('resize', scheduleLayoutRefresh);

    return () => {
      chartHostEl.removeEventListener('wheel', handleWheel, true);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
      window.removeEventListener('focus', scheduleLayoutRefresh);
      window.removeEventListener('resize', scheduleLayoutRefresh);

      if (layoutRefreshFrame) {
        window.cancelAnimationFrame(layoutRefreshFrame);
      }

      if (layoutRefreshTimeout !== null) {
        window.clearTimeout(layoutRefreshTimeout);
      }

      clearWheelGestureReset();
    };
  });

  $: controller.setViewportKey(viewportKey);
  $: controller.setInitialVisibleBars(initialVisibleBars);
  $: controller.setData(bars);
  $: if (activeOverlay) {
    controller.updateOverlay(activeOverlay.bar);
  }
</script>

<div class="chart-canvas">
  <div bind:this={chartHostEl} class="chart-canvas__host"></div>
  {#if showOverlay}
    <div class="chart-canvas__surface">
      <div class="chart-canvas__placeholder">{headline}</div>
      <div class="chart-canvas__meta">{detail}</div>
      {#if status === 'building'}
        <div class="chart-canvas__status chart-canvas__status--building">构建中</div>
      {:else if status === 'failed'}
        <div class="chart-canvas__status chart-canvas__status--failed">状态异常</div>
      {:else if status === 'loading'}
        <div class="chart-canvas__status">加载中</div>
      {/if}
    </div>
  {/if}
</div>
