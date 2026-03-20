type DisplayTargetType = 'index' | 'board' | 'symbol';
type DisplayGranularity = 'day' | 'week';

export interface ChartSourceHintInput {
  targetType: DisplayTargetType;
  granularity: DisplayGranularity;
  sourceStatus: string;
  providerKind?: string;
  providerSymbol?: string;
  valueMode?: string;
}

const DEV_COPY_REPLACEMENTS: Array<[RegExp, string]> = [
  [/本地样例/g, ''],
  [/[（(]\s*mock\s*[）)]/giu, ''],
  [/\(\s*mock\s*\)/giu, ''],
  [/\bmock\b/giu, ''],
  [/\bfallback\b/giu, ''],
  [/\bfixture\b/giu, ''],
];

export function sanitizeUserFacingMessage(message: string): string {
  if (!message.trim()) {
    return '';
  }

  const exactMatch = new Map<string, string>([
    ['本地缓存可读', '缓存可读'],
    ['本地样例鉴权已保存', '鉴权已保存'],
    ['本地样例同步完成', '同步完成'],
    ['本地样例启动同步完成', '启动同步完成'],
    ['手动同步完成（mock）', '手动同步完成'],
    ['启动同步完成（mock）', '启动同步完成'],
  ]);
  const exact = exactMatch.get(message.trim());

  if (exact) {
    return exact;
  }

  let nextMessage = message;
  for (const [pattern, replacement] of DEV_COPY_REPLACEMENTS) {
    nextMessage = nextMessage.replace(pattern, replacement);
  }

  return nextMessage
    .replace(/\s{2,}/g, ' ')
    .replace(/[（(]\s*[·,，/]\s*[)）]/g, '')
    .replace(/^[·,，/、\s]+|[·,，/、\s]+$/g, '')
    .trim();
}

export function formatSourceStatusLabel(sourceStatus: string): string {
  const normalized = sourceStatus.trim();

  switch (normalized) {
    case '':
      return '';
    case 'local_cache':
      return '本地缓存';
    case 'proxy_etf_cache':
      return '代理指数缓存';
    case 'live_quote':
      return '实时报价';
    case 'market_closed':
      return '已收盘';
    case 'historical_fallback':
      return '历史收盘数据';
    case 'sqlite_fixture':
      return '历史价格';
    case 'local_fixture_points':
      return '历史价格';
    default:
      return sanitizeUserFacingMessage(normalized) || normalized;
  }
}

export function formatChartSourceHint(input: ChartSourceHintInput): string {
  const basisLabel = buildPricingBasisLabel(input.targetType, input.granularity);
  const providerLabel = buildProviderLabel(input.providerKind, input.providerSymbol, input.valueMode);
  const sourceLabel = providerLabel || buildSourceLabel(input.sourceStatus);

  return [basisLabel, sourceLabel].filter(Boolean).join(' · ');
}

function buildPricingBasisLabel(targetType: DisplayTargetType, granularity: DisplayGranularity): string {
  const granularityLabel = granularity === 'week' ? '周K' : '日K';

  if (targetType === 'board') {
    return `口径：基于前复权成分股合成 · ${granularityLabel}`;
  }

  return `口径：前复权${granularityLabel}`;
}

function buildProviderLabel(providerKind?: string, providerSymbol?: string, valueMode?: string): string {
  if (!providerKind && !providerSymbol) {
    return '';
  }

  if (providerKind === 'proxy_etf' && providerSymbol) {
    return `数据源：${providerSymbol}（代理指数）`;
  }

  if (providerSymbol) {
    const providerKindLabel = formatProviderKind(providerKind, valueMode);
    return providerKindLabel ? `数据源：${providerSymbol}（${providerKindLabel}）` : `数据源：${providerSymbol}`;
  }

  const providerKindLabel = formatProviderKind(providerKind, valueMode);
  return providerKindLabel ? `数据源：${providerKindLabel}` : '';
}

function buildSourceLabel(sourceStatus: string): string {
  const label = formatSourceStatusLabel(sourceStatus);
  return label ? `数据源：${label}` : '';
}

function formatProviderKind(providerKind?: string, valueMode?: string): string {
  if (!providerKind) {
    if (valueMode === 'local_fixture_points') {
      return '历史价格';
    }

    return '';
  }

  switch (providerKind) {
    case 'proxy_etf':
      return '代理指数';
    case 'sqlite_fixture':
      return '历史价格';
    case 'historical_fallback':
      return '历史收盘数据';
    case 'longbridge':
      return 'Longbridge';
    default:
      return providerKind
        .split(/[_-]+/g)
        .filter(Boolean)
        .map((segment, index) => (index === 0 ? capitalize(segment) : segment))
        .join(' ');
  }
}

function capitalize(value: string): string {
  return value ? `${value[0].toUpperCase()}${value.slice(1)}` : value;
}
