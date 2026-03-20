import {
  CandlestickSeries,
  ColorType,
  createChart,
  type CandlestickData,
  type IChartApi,
  type ISeriesApi,
  type Time,
} from 'lightweight-charts';
import type { ActiveOverlayPayload, BarPoint } from '../types/contracts';
import {
  createFollowLatestState,
  resolveFollowLatestStateOnRangeChange,
  resolveFollowLatestStateOnViewportChange,
  withAnchorScrollPosition,
} from './followLatestState';
import { resolveVisiblePriceScaleRange } from './visiblePriceScale';
import { bumpRuntimeCounter, recordRuntimeSignal, setRuntimeSnapshot } from '../services/runtimeDiagnostics';
import { setChartViewportAutoFollowLatest } from '../stores/chartViewportStore';

const CONTROLLER_INSTANCE_ID = 'lightweight-singleton-chart-v1';
const DEFAULT_INITIAL_VISIBLE_BARS = 132;
const DEFAULT_INITIAL_RIGHT_OFFSET_BARS = 4;
const MIN_INITIAL_VISIBLE_BARS = 12;
const MIN_ZOOM_VISIBLE_BARS = 24;
const MIN_WHEEL_DELTA = 0.01;
const HORIZONTAL_PAN_SENSITIVITY = 0.9;
const MANUAL_PRICE_SCALE_PADDING_RATIO = 0.06;

export class LightweightChartController {
  private container: HTMLElement | null = null;
  private chart: IChartApi | null = null;
  private series: ISeriesApi<'Candlestick', Time> | null = null;
  private bars: BarPoint[] = [];
  private deferredOverlayBar: ActiveOverlayPayload['bar'] | null = null;
  private lastRenderedAutoFollowLatest: boolean | null = null;
  private mountCount = 0;
  private setDataCount = 0;
  private overlayUpdateCount = 0;
  private lastDataSignature = '';
  private followLatestState = createFollowLatestState();
  private isApplyingProgrammaticRange = false;
  private initialVisibleBars = DEFAULT_INITIAL_VISIBLE_BARS;
  private pendingInitialViewport = true;

  setViewportKey(viewportKey: string): void {
    const viewportChanged = this.followLatestState.viewportKey !== viewportKey;
    this.followLatestState = resolveFollowLatestStateOnViewportChange(this.followLatestState, viewportKey);

    if (viewportChanged) {
      this.deferredOverlayBar = null;
      this.pendingInitialViewport = true;
    }

    this.renderDiagnostics();
  }

  setInitialVisibleBars(initialVisibleBars: number): void {
    const nextVisibleBars = Math.max(MIN_INITIAL_VISIBLE_BARS, Math.floor(initialVisibleBars));

    if (this.initialVisibleBars === nextVisibleBars) {
      return;
    }

    this.initialVisibleBars = nextVisibleBars;
    this.pendingInitialViewport = true;
    this.renderDiagnostics();
  }

  mount(container: HTMLElement): void {
    if (this.container === container && this.chart && this.series) {
      return;
    }

    this.detachTimeScaleListener();

    if (this.chart) {
      this.chart.remove();
      this.chart = null;
      this.series = null;
    }

    this.container = container;
    this.lastRenderedAutoFollowLatest = null;
    this.mountCount += 1;
    recordRuntimeSignal('chart.controller.mount', {
      instanceId: CONTROLLER_INSTANCE_ID,
      mountCount: this.mountCount,
    });
    this.chart = createChart(container, {
      autoSize: true,
      layout: {
        background: {
          type: ColorType.Solid,
          color: readToken('--bg-surface', '#fcfcfd'),
        },
        textColor: readToken('--text-secondary', '#697385'),
        fontSize: 12,
        fontFamily: "'SF Pro Text', 'Helvetica Neue', Arial, sans-serif",
        attributionLogo: false,
      },
      grid: {
        vertLines: {
          color: readToken('--line-soft', '#eff2f6'),
          visible: false,
        },
        horzLines: {
          color: readToken('--line-soft', '#eff2f6'),
        },
      },
      handleScroll: {
        mouseWheel: false,
        pressedMouseMove: true,
        horzTouchDrag: true,
        vertTouchDrag: false,
      },
      handleScale: {
        axisPressedMouseMove: false,
        mouseWheel: false,
        pinch: false,
      },
      crosshair: {
        vertLine: {
          color: 'rgba(127, 168, 243, 0.32)',
          width: 1,
          labelBackgroundColor: readToken('--accent-blue', '#7fa8f3'),
        },
        horzLine: {
          color: 'rgba(127, 168, 243, 0.18)',
          width: 1,
          labelBackgroundColor: readToken('--accent-blue', '#7fa8f3'),
        },
      },
      rightPriceScale: {
        borderColor: readToken('--line-subtle', '#e6e8ee'),
        scaleMargins: {
          top: 0.12,
          bottom: 0.08,
        },
      },
      timeScale: {
        borderColor: readToken('--line-subtle', '#e6e8ee'),
        timeVisible: false,
        secondsVisible: false,
        fixLeftEdge: true,
        lockVisibleTimeRangeOnResize: true,
        rightOffset: 4,
        barSpacing: 10,
      },
    });
    this.series = this.chart.addSeries(CandlestickSeries, {
      upColor: readToken('--chart-up', '#2f8f6a'),
      downColor: readToken('--chart-down', '#c84d5f'),
      borderVisible: false,
      wickUpColor: readToken('--chart-up', '#2f8f6a'),
      wickDownColor: readToken('--chart-down', '#c84d5f'),
      priceLineVisible: true,
      lastValueVisible: true,
    });
    this.attachTimeScaleListener();
    this.followLatestState = withAnchorScrollPosition(this.followLatestState, null);

    this.applyBars(true);
  }

  setData(bars: BarPoint[]): void {
    this.bars = [...bars];
    this.deferredOverlayBar = null;
    this.setDataCount += 1;
    bumpRuntimeCounter('chart.controller.set-data', {
      bars: this.bars.length,
    });
    this.applyBars(true);
    this.refreshLayout();
  }

  updateOverlay(bar: ActiveOverlayPayload['bar']): void {
    this.bars = this.bars.length === 0 ? [bar] : [...this.bars.slice(0, -1), bar];
    this.overlayUpdateCount += 1;
    bumpRuntimeCounter('chart.controller.update-overlay', {
      bars: this.bars.length,
      tradeDate: bar.time,
    });

    if (!this.followLatestState.autoFollowLatest) {
      this.deferredOverlayBar = bar;
      this.renderDiagnostics();
      return;
    }

    this.applyOverlay(bar);
  }

  refreshLayout(): void {
    if (!this.chart || !this.container) {
      return;
    }

    const width = Math.max(1, Math.floor(this.container.clientWidth));
    const height = Math.max(1, Math.floor(this.container.clientHeight));

    this.chart.resize(width, height, true);

    if (this.pendingInitialViewport && this.bars.length > 0) {
      this.focusRecentBarsAndCaptureAnchor();
      this.pendingInitialViewport = false;
    } else if (this.followLatestState.autoFollowLatest) {
      this.scrollToAnchorPosition();
    }

    this.renderDiagnostics();
  }

  applyProportionalZoom(deltaY: number, clientX?: number): void {
    if (!this.chart || !this.container || this.bars.length === 0 || Math.abs(deltaY) < MIN_WHEEL_DELTA) {
      return;
    }

    const visibleRange = this.chart.timeScale().getVisibleLogicalRange();

    if (!visibleRange) {
      return;
    }

    const currentLength = Math.max(1, visibleRange.to - visibleRange.from);
    const maxVisibleBars = Math.max(this.initialVisibleBars, this.bars.length + DEFAULT_INITIAL_RIGHT_OFFSET_BARS * 2);
    const zoomFactor = clamp(Math.exp(clamp(deltaY, -160, 160) * 0.0015), 0.92, 1.08);
    const nextLength = clamp(currentLength * zoomFactor, MIN_ZOOM_VISIBLE_BARS, maxVisibleBars);
    const rect = this.container.getBoundingClientRect();
    const anchorRatio = rect.width > 0 && clientX !== undefined
      ? clamp((clientX - rect.left) / rect.width, 0, 1)
      : 0.5;
    const anchorLogicalIndex = visibleRange.from + currentLength * anchorRatio;
    const from = anchorLogicalIndex - nextLength * anchorRatio;
    const to = from + nextLength;

    this.pendingInitialViewport = false;
    this.enterManualBrowseMode();
    this.applyProgrammaticRangeChange(() => {
      this.chart?.timeScale().setVisibleLogicalRange({ from, to });
    });
    this.renderDiagnostics();
  }

  applyHorizontalPan(deltaX: number): void {
    if (!this.chart || !this.container || this.bars.length === 0 || Math.abs(deltaX) < MIN_WHEEL_DELTA) {
      return;
    }

    const visibleRange = this.chart.timeScale().getVisibleLogicalRange();

    if (!visibleRange) {
      return;
    }

    const rect = this.container.getBoundingClientRect();

    if (rect.width <= 0) {
      return;
    }

    const currentLength = Math.max(1, visibleRange.to - visibleRange.from);
    const deltaBars = (deltaX / rect.width) * currentLength * HORIZONTAL_PAN_SENSITIVITY;

    if (Math.abs(deltaBars) < MIN_WHEEL_DELTA) {
      return;
    }

    const from = visibleRange.from + deltaBars;
    const to = visibleRange.to + deltaBars;

    this.pendingInitialViewport = false;
    this.enterManualBrowseMode();
    this.applyProgrammaticRangeChange(() => {
      this.chart?.timeScale().setVisibleLogicalRange({ from, to });
    });
    this.renderDiagnostics();
  }

  private applyBars(shouldFitContent: boolean): void {
    if (!this.series) {
      this.renderDiagnostics();
      return;
    }

    const nextBars = this.bars.map(toCandlestickData);
    this.series.setData(nextBars);

    const nextSignature = buildDataSignature(this.bars);
    if (shouldFitContent && nextBars.length > 0 && nextSignature !== this.lastDataSignature) {
      if (this.followLatestState.anchorScrollPosition === null) {
        this.focusRecentBarsAndCaptureAnchor();
      } else if (this.followLatestState.autoFollowLatest) {
        this.scrollToAnchorPosition();
      }
    }

    this.lastDataSignature = nextSignature;
    this.renderDiagnostics();
  }

  private applyOverlay(bar: ActiveOverlayPayload['bar']): void {
    if (!this.series) {
      this.renderDiagnostics();
      return;
    }

    this.series.update(toCandlestickData(bar));
    if (this.followLatestState.autoFollowLatest) {
      this.scrollToAnchorPosition();
    }
    this.lastDataSignature = buildDataSignature(this.bars);
    this.renderDiagnostics();
  }

  private attachTimeScaleListener(): void {
    this.chart?.timeScale().subscribeVisibleLogicalRangeChange(this.handleVisibleLogicalRangeChange);
  }

  private detachTimeScaleListener(): void {
    this.chart?.timeScale().unsubscribeVisibleLogicalRangeChange(this.handleVisibleLogicalRangeChange);
  }

  private readonly handleVisibleLogicalRangeChange = (): void => {
    const previousAutoFollowLatest = this.followLatestState.autoFollowLatest;
    const scrollPosition = this.chart?.timeScale().scrollPosition();
    const autoFollowLatestFromVisibleRange = this.resolveAutoFollowLatestFromVisibleRange();

    if (scrollPosition === undefined) {
      return;
    }

    this.followLatestState = resolveFollowLatestStateOnRangeChange(this.followLatestState, {
      scrollPosition,
      isProgrammaticChange: this.isApplyingProgrammaticRange,
    });

    if (autoFollowLatestFromVisibleRange !== null && autoFollowLatestFromVisibleRange !== this.followLatestState.autoFollowLatest) {
      this.followLatestState = {
        ...this.followLatestState,
        autoFollowLatest: autoFollowLatestFromVisibleRange,
      };
    }

    if (!previousAutoFollowLatest && this.followLatestState.autoFollowLatest) {
      this.flushDeferredOverlayIfNeeded();
    }

    this.renderDiagnostics();
  };

  private focusRecentBarsAndCaptureAnchor(): void {
    if (!this.chart || this.bars.length === 0) {
      return;
    }

    const lastIndex = Math.max(0, this.bars.length - 1);
    const startIndex = Math.max(0, this.bars.length - this.initialVisibleBars);
    const from = this.bars[startIndex]?.time;
    const to = this.bars[lastIndex]?.time;

    if (!from || !to) {
      return;
    }

    this.applyProgrammaticRangeChange(() => {
      this.chart?.timeScale().setVisibleRange({ from, to });
      this.chart?.timeScale().scrollToPosition(DEFAULT_INITIAL_RIGHT_OFFSET_BARS, false);
    });
    this.captureAnchorScrollPosition();
  }

  private enterManualBrowseMode(): void {
    if (!this.followLatestState.autoFollowLatest) {
      return;
    }

    this.followLatestState = {
      ...this.followLatestState,
      autoFollowLatest: false,
    };
  }

  private scrollToAnchorPosition(): void {
    const anchorScrollPosition = this.followLatestState.anchorScrollPosition;

    if (anchorScrollPosition === null) {
      this.focusRecentBarsAndCaptureAnchor();
      return;
    }

    this.applyProgrammaticRangeChange(() => {
      this.chart?.timeScale().scrollToPosition(anchorScrollPosition, false);
    });
    this.captureAnchorScrollPosition();
    this.flushDeferredOverlayIfNeeded();
  }

  private applyProgrammaticRangeChange(action: () => void): void {
    this.isApplyingProgrammaticRange = true;

    try {
      action();
    } finally {
      this.isApplyingProgrammaticRange = false;
    }
  }

  private captureAnchorScrollPosition(): void {
    const scrollPosition = this.chart?.timeScale().scrollPosition();

    if (scrollPosition === undefined) {
      return;
    }

    this.followLatestState = withAnchorScrollPosition(this.followLatestState, scrollPosition);
  }

  private resolveAutoFollowLatestFromVisibleRange(): boolean | null {
    if (!this.chart || this.bars.length === 0) {
      return null;
    }

    const visibleLogicalRange = this.chart.timeScale().getVisibleLogicalRange();

    if (!visibleLogicalRange) {
      return null;
    }

    const latestLogicalIndex = this.bars.length - 1 + DEFAULT_INITIAL_RIGHT_OFFSET_BARS;
    return visibleLogicalRange.to >= latestLogicalIndex - 0.5;
  }

  private flushDeferredOverlayIfNeeded(): void {
    if (!this.followLatestState.autoFollowLatest || !this.deferredOverlayBar) {
      return;
    }

    const deferredOverlayBar = this.deferredOverlayBar;
    this.deferredOverlayBar = null;
    this.applyOverlay(deferredOverlayBar);
  }

  private renderDiagnostics(): void {
    this.syncFollowLatestPresentation();
    this.syncVisiblePriceScale();
    setChartViewportAutoFollowLatest(this.followLatestState.autoFollowLatest);

    if (!this.container) {
      return;
    }

    this.container.dataset.controller = 'lightweight-singleton-chart';
    this.container.dataset.controllerInstanceId = CONTROLLER_INSTANCE_ID;
    this.container.dataset.mountCount = String(this.mountCount);
    this.container.dataset.setDataCount = String(this.setDataCount);
    this.container.dataset.overlayUpdateCount = String(this.overlayUpdateCount);
    this.container.dataset.bars = String(this.bars.length);
    this.container.dataset.autoFollowLatest = String(this.followLatestState.autoFollowLatest);
    this.container.dataset.anchorScrollPosition = this.followLatestState.anchorScrollPosition === null
      ? ''
      : String(this.followLatestState.anchorScrollPosition);
    this.container.dataset.viewportKey = this.followLatestState.viewportKey;
    this.container.dataset.pendingInitialViewport = String(this.pendingInitialViewport);
    setRuntimeSnapshot('chart.controller', {
      instanceId: CONTROLLER_INSTANCE_ID,
      mountCount: this.mountCount,
      setDataCount: this.setDataCount,
      overlayUpdateCount: this.overlayUpdateCount,
      bars: this.bars.length,
      autoFollowLatest: this.followLatestState.autoFollowLatest,
      anchorScrollPosition: this.followLatestState.anchorScrollPosition,
      viewportKey: this.followLatestState.viewportKey,
      pendingInitialViewport: this.pendingInitialViewport,
    });
  }

  private syncFollowLatestPresentation(): void {
    if (!this.series || this.lastRenderedAutoFollowLatest === this.followLatestState.autoFollowLatest) {
      return;
    }

    this.series.applyOptions({
      priceLineVisible: this.followLatestState.autoFollowLatest,
      lastValueVisible: this.followLatestState.autoFollowLatest,
    });
    this.lastRenderedAutoFollowLatest = this.followLatestState.autoFollowLatest;
  }

  private syncVisiblePriceScale(): void {
    if (!this.chart || !this.series) {
      return;
    }

    const visiblePriceScaleRange = resolveVisiblePriceScaleRange(
      this.bars,
      this.chart.timeScale().getVisibleLogicalRange(),
      MANUAL_PRICE_SCALE_PADDING_RATIO,
    );

    if (!visiblePriceScaleRange) {
      return;
    }

    this.series.priceScale().setVisibleRange(visiblePriceScaleRange);
  }
}

export const lightweightChartController = new LightweightChartController();

function toCandlestickData(bar: BarPoint): CandlestickData<Time> {
  return {
    time: bar.time,
    open: bar.open,
    high: bar.high,
    low: bar.low,
    close: bar.close,
  };
}

function buildDataSignature(bars: BarPoint[]): string {
  if (bars.length === 0) {
    return 'empty';
  }

  const first = bars[0];
  const last = bars[bars.length - 1];
  return `${bars.length}:${first.time}:${last.time}:${last.close}`;
}

function readToken(name: string, fallback: string): string {
  if (typeof window === 'undefined') {
    return fallback;
  }

  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value || fallback;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}
