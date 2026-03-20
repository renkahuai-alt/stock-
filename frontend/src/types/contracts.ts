export type TargetType = 'index' | 'board' | 'symbol';
export type Granularity = 'day' | 'week';
export type RangeKey = '1m' | '3m' | '6m' | '1y' | '3y' | 'all';
export type BoardAlgorithm = 'equal_weight_v1' | 'market_cap_weight_v1';
export type MarketState = 'open' | 'closed';
export type BuildStatus = 'queued' | 'running' | 'succeeded' | 'failed';
export type BuildPhase =
  | 'queued'
  | 'fetching_symbols'
  | 'fetching_history'
  | 'recomputing_board'
  | 'persisting'
  | 'completed'
  | 'failed';
export type SyncStatus =
  | 'no_credentials'
  | 'first_sync_running'
  | 'incremental_sync_running'
  | 'ready'
  | 'offline_readable'
  | 'sync_failed'
  | 'chart_empty';
export type RunSyncMode = 'startup' | 'manual';
export type SettingsSection = 'credentials' | 'boards';

export interface IndexItem {
  id: string;
  label: string;
  disabled?: boolean;
}

export interface BoardSummary {
  boardId: string;
  name: string;
  compositionAlgorithm: BoardAlgorithm;
  buildStatus: BuildStatus;
  buildPhase: BuildPhase;
  buildTotal: number;
  buildCompleted: number;
  buildFailed: number;
  buildJobId?: string;
  buildMessage?: string;
  updatedAt: string;
}

export interface MemberSummary {
  symbol: string;
  weightPercent?: number;
}

export interface TargetNotePayload {
  targetType: TargetType;
  targetId: string;
  content: string;
  updatedAt: string | null;
}

export interface GetTargetNoteRequest {
  targetType: TargetType;
  targetId: string;
}

export interface GetBoardMemberSummariesRequest {
  boardId: string;
  compositionAlgorithm: BoardAlgorithm;
}

export interface SaveCredentialsPayload {
  appKey: string;
  appSecret: string;
  accessToken: string;
}

export interface BarPoint {
  time: string;
  open: number;
  high: number;
  low: number;
  close: number;
  volume?: number;
}

export interface ChartMeta {
  targetType: TargetType;
  targetId: string;
  title: string;
  sourceStatus: string;
  providerSymbol?: string;
  providerKind?: string;
  valueMode?: string;
  granularity?: Granularity;
  range?: RangeKey;
}

export interface ActiveOverlayPayload {
  kind: 'current_day';
  bar: BarPoint;
}

export interface LiveOverlayBar {
  tradeDate: string;
  open: number;
  high: number;
  low: number;
  close: number;
  volume?: number;
}

export interface ChartLiveMeta {
  title?: string;
  sourceStatus?: string;
  providerSymbol?: string;
  providerKind?: string;
  valueMode?: string;
  message?: string;
}

export interface ChartPayload {
  meta: ChartMeta;
  bars: BarPoint[];
  latestTradeDate: string | null;
  sourceStatus: string;
  activeOverlay?: ActiveOverlayPayload;
}

export interface ChartLiveUpdatePayload {
  watchId: string;
  targetType: TargetType;
  targetId: string;
  granularity: 'day';
  boardAlgorithm?: BoardAlgorithm;
  updatedAt: string;
  marketState: MarketState;
  sourceStatus: string;
  overlay: LiveOverlayBar;
  meta: ChartLiveMeta;
}

export interface SyncStatusPayload {
  status: SyncStatus;
  message: string;
  lastSyncAt: string | null;
  latestTradeDate: string | null;
}

export interface BootstrapPayload {
  indexes: IndexItem[];
  boards: BoardSummary[];
  membersByBoard: Record<string, MemberSummary[]>;
  activeTargetNote: TargetNotePayload;
  syncStatus: SyncStatusPayload;
}

export interface BoardMemberSummariesPayload {
  boardId: string;
  compositionAlgorithm: BoardAlgorithm;
  members: MemberSummary[];
  updatedAt: string;
}

export interface GetChartRequest {
  targetType: TargetType;
  targetId: string;
  granularity: Granularity;
  range: RangeKey;
  boardAlgorithm?: BoardAlgorithm;
}

export interface StartChartWatchRequest {
  targetType: TargetType;
  targetId: string;
  granularity: 'day';
  boardAlgorithm?: BoardAlgorithm;
}

export interface ChartWatchStatusPayload {
  watchId: string;
  started: boolean;
  targetType: TargetType;
  targetId: string;
  granularity: 'day';
  boardAlgorithm?: BoardAlgorithm;
  intervalSec: number;
  marketState: MarketState;
  updatedAt: string;
  message?: string;
}

export interface StopChartWatchPayload {
  stopped: boolean;
  watchId?: string;
  updatedAt: string;
}

export interface SaveBoardRequest {
  boardId?: string;
  name: string;
  members: string[];
  compositionAlgorithm: BoardAlgorithm;
}

export interface SaveBoardResponse {
  boardId: string;
  rebuildStarted: boolean;
  backgroundSyncStarted: boolean;
  buildStatus: BuildStatus;
  buildPhase: BuildPhase;
  buildJobId?: string;
  compositionAlgorithm: BoardAlgorithm;
}

export interface BoardBuildStatusPayload {
  boardId: string;
  name: string;
  buildStatus: BuildStatus;
  buildPhase: BuildPhase;
  buildTotal: number;
  buildCompleted: number;
  buildFailed: number;
  buildJobId?: string;
  buildMessage?: string;
  updatedAt: string;
}
