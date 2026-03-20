import type {
  BoardBuildStatusPayload,
  BootstrapPayload,
  ChartPayload,
  GetTargetNoteRequest,
  RunSyncMode,
  SaveBoardRequest,
  SaveBoardResponse,
  SyncStatusPayload,
  TargetNotePayload,
  TargetType,
} from '../types/contracts';

export const mockSyncStatus: SyncStatusPayload = {
  status: 'ready',
  message: '缓存可读',
  lastSyncAt: '2026-03-19T09:30:00+08:00',
  latestTradeDate: '2026-03-18',
};

export const mockBootstrap: BootstrapPayload = {
  indexes: [
    { id: 'DJI', label: '道琼斯' },
    { id: 'IXIC', label: '纳斯达克' },
    { id: 'GSPC', label: '标普500' },
    { id: 'RUT', label: '罗素2000' },
  ],
  boards: [
    {
      boardId: 'board-ai',
      name: 'AI半导体',
      compositionAlgorithm: 'equal_weight_v1',
      buildStatus: 'succeeded',
      buildPhase: 'completed',
      buildTotal: 6,
      buildCompleted: 6,
      buildFailed: 0,
      updatedAt: '2026-03-19T09:31:00+08:00',
    },
    {
      boardId: 'board-cloud',
      name: '云软件',
      compositionAlgorithm: 'market_cap_weight_v1',
      buildStatus: 'running',
      buildPhase: 'fetching_history',
      buildTotal: 12,
      buildCompleted: 8,
      buildFailed: 0,
      buildJobId: 'job-board-cloud',
      buildMessage: '后台构建中',
      updatedAt: '2026-03-19T09:33:00+08:00',
    },
    {
      boardId: 'board-energy',
      name: '核能产业链',
      compositionAlgorithm: 'equal_weight_v1',
      buildStatus: 'failed',
      buildPhase: 'failed',
      buildTotal: 5,
      buildCompleted: 3,
      buildFailed: 2,
      buildJobId: 'job-board-energy',
      buildMessage: '部分成分股历史缺失',
      updatedAt: '2026-03-19T09:35:00+08:00',
    },
  ],
  membersByBoard: {
    'board-ai': [
      { symbol: 'NVDA', weightPercent: 30.2 },
      { symbol: 'AMD', weightPercent: 18.1 },
      { symbol: 'AVGO', weightPercent: 17.5 },
      { symbol: 'TSM', weightPercent: 16.7 },
      { symbol: 'ASML', weightPercent: 9.4 },
      { symbol: 'ARM', weightPercent: 8.1 },
    ],
    'board-cloud': [
      { symbol: 'MSFT', weightPercent: 29.1 },
      { symbol: 'AMZN', weightPercent: 24.3 },
      { symbol: 'CRM', weightPercent: 12.6 },
      { symbol: 'SNOW', weightPercent: 11.2 },
    ],
    'board-energy': [
      { symbol: 'CCJ', weightPercent: 33.3 },
      { symbol: 'BWXT', weightPercent: 23.1 },
      { symbol: 'SMR', weightPercent: 21.7 },
    ],
  },
  activeTargetNote: {
    targetType: 'board',
    targetId: 'board-ai',
    content: '记录板块假设、催化剂与风险点，先冻结输入区结构。',
    updatedAt: '2026-03-19T09:32:00+08:00',
  },
  syncStatus: mockSyncStatus,
};

export function makeMockChart(title: string, targetType: TargetType = 'board', targetId = 'board-ai'): ChartPayload {
  const isIndex = targetType === 'index';
  const resolvedTitle =
    targetId === 'board-ai'
      ? 'AI半导体'
      : targetId === 'board-cloud'
        ? '云软件'
        : targetId === 'board-energy'
          ? '核能产业链'
          : title;

  return {
    meta: {
      targetType,
      targetId,
      title: resolvedTitle,
      sourceStatus: isIndex ? 'proxy_etf_cache' : 'local_cache',
      providerSymbol: isIndex ? 'ONEQ.US' : undefined,
      providerKind: isIndex ? 'proxy_etf' : undefined,
      valueMode: isIndex ? 'proxy_scaled_index_points' : undefined,
      granularity: 'day',
      range: '1y',
    },
    bars: [
      { time: '2026-03-16', open: 100, high: 104, low: 99, close: 103, volume: 1200 },
      { time: '2026-03-17', open: 103, high: 106, low: 102, close: 105, volume: 1420 },
      { time: '2026-03-18', open: 105, high: 107, low: 104, close: 106, volume: 1660 },
    ],
    latestTradeDate: '2026-03-18',
    sourceStatus: isIndex ? 'proxy_etf_cache' : 'local_cache',
  };
}

export function makeBoardBuildStatus(boardId: string): BoardBuildStatusPayload {
  return {
    boardId,
    name: '新建板块',
    buildStatus: 'queued',
    buildPhase: 'queued',
    buildTotal: 0,
    buildCompleted: 0,
    buildFailed: 0,
    buildJobId: `job-${boardId}`,
    buildMessage: '等待后台构建',
    updatedAt: new Date().toISOString(),
  };
}

export function makeTargetNote(payload: GetTargetNoteRequest): TargetNotePayload {
  return {
    targetType: payload.targetType,
    targetId: payload.targetId,
    content: '',
    updatedAt: null,
  };
}

export function makeRunSyncStatus(mode: RunSyncMode): SyncStatusPayload {
  return {
    ...mockSyncStatus,
    message: mode === 'manual' ? '手动同步完成' : '启动同步完成',
    lastSyncAt: '2026-03-19T10:00:00+08:00',
  };
}

export function saveBoardFallback(payload: SaveBoardRequest): SaveBoardResponse {
  const boardId = payload.boardId ?? `board-${payload.name.toLowerCase().replace(/\s+/g, '-')}`;
  const backgroundSyncStarted = payload.members.length > 20;

  return {
    boardId,
    rebuildStarted: true,
    backgroundSyncStarted,
    buildStatus: backgroundSyncStarted ? 'queued' : 'succeeded',
    buildPhase: backgroundSyncStarted ? 'queued' : 'completed',
    buildJobId: backgroundSyncStarted ? `job-${boardId}` : undefined,
    compositionAlgorithm: payload.compositionAlgorithm,
  };
}
