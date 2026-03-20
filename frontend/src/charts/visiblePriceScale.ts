import type { BarPoint } from '../types/contracts';

export interface VisibleLogicalRange {
  from: number;
  to: number;
}

export interface VisiblePriceScaleRange {
  from: number;
  to: number;
}

export function resolveVisiblePriceScaleRange(
  bars: BarPoint[],
  visibleLogicalRange: VisibleLogicalRange | null,
  paddingRatio: number,
): VisiblePriceScaleRange | null {
  if (!visibleLogicalRange || bars.length === 0) {
    return null;
  }

  const fromIndex = clamp(Math.floor(visibleLogicalRange.from), 0, bars.length - 1);
  const toIndex = clamp(Math.ceil(visibleLogicalRange.to), fromIndex, bars.length - 1);
  const visibleBars = bars.slice(fromIndex, toIndex + 1);

  if (visibleBars.length === 0) {
    return null;
  }

  let minPrice = Number.POSITIVE_INFINITY;
  let maxPrice = Number.NEGATIVE_INFINITY;

  for (const bar of visibleBars) {
    minPrice = Math.min(minPrice, bar.low);
    maxPrice = Math.max(maxPrice, bar.high);
  }

  if (!Number.isFinite(minPrice) || !Number.isFinite(maxPrice)) {
    return null;
  }

  const priceSpan = Math.max(maxPrice - minPrice, Math.max(Math.abs(maxPrice), 1) * 0.001);
  const padding = priceSpan * paddingRatio;

  return {
    from: minPrice - padding,
    to: maxPrice + padding,
  };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}
